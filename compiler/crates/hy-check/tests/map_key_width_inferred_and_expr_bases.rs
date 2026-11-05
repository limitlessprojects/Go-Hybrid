//! Regression tests for map key width checking on inferred local map bindings,
//! non-identifier map base expressions, map type aliases, and shadowed map bindings.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check_errors(src: &str) -> Vec<(String, String)> {
    let ast = parse_file("test.hy", src).expect("test snippet must parse");
    let res = check_file(&ast);
    res.diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| (d.code, d.message))
        .collect()
}

#[test]
fn rejects_mismatched_key_width_on_short_declared_maps() {
    for (src, expected_code) in [
        (
            r#"
fn main() {
    m := map[int32]string{1: "a"}
    var k int64 = 1
    _ = m[k]
}
"#,
            "HYB-E1002",
        ),
        (
            r#"
fn main() {
    m := map[int32]string{1: "a"}
    var k int64 = 1
    m[k] = "b"
    _ = m
}
"#,
            "HYB-E1002",
        ),
        (
            r#"
fn main() {
    m := map[int32]string{1: "a"}
    var k int64 = 1
    delete(m, k)
}
"#,
            "HYB-E2005",
        ),
        (
            r#"
fn main() {
    m := map[int32]string{}
    var k int64 = 1
    _ = m[k]
}
"#,
            "HYB-E1002",
        ),
        (
            r#"
fn main() {
    m := map[int32]string{}
    var k int64 = 1
    delete(m, k)
}
"#,
            "HYB-E2005",
        ),
    ] {
        let errs = check_errors(src);
        assert_eq!(errs.len(), 1, "expected exactly 1 error for:\n{src}\ngot: {errs:?}");
        assert_eq!(errs[0].0, expected_code, "unexpected code for:\n{src}\ngot: {errs:?}");
    }
}

#[test]
fn rejects_mismatched_key_width_on_non_ident_bases_and_type_aliases() {
    for (src, expected_code) in [
        (
            r#"
fn main() {
    var k int64 = 1
    _ = map[int32]string{1: "a"}[k]
}
"#,
            "HYB-E1002",
        ),
        (
            r#"
fn main() {
    var k int64 = 1
    delete(map[int32]string{1: "a"}, k)
}
"#,
            "HYB-E2005",
        ),
        (
            r#"
fn main() {
    var k int64 = 1
    _ = (map[int32]string{})[k]
}
"#,
            "HYB-E1002",
        ),
        (
            r#"
struct holder {
    m map[int32]string
}

fn main() {
    h := holder{ m: map[int32]string{1: "a"} }
    var k int64 = 1
    _ = h.m[k]
}
"#,
            "HYB-E1002",
        ),
        (
            r#"
struct holder {
    m map[int32]string
}

fn main() {
    h := holder{ m: map[int32]string{1: "a"} }
    var k int64 = 1
    delete(h.m, k)
}
"#,
            "HYB-E2005",
        ),
        (
            r#"
fn make_table() map[int32]string {
    return map[int32]string{1: "a"}
}

fn main() {
    var k int64 = 1
    _ = make_table()[k]
}
"#,
            "HYB-E1002",
        ),
        (
            r#"
type table = map[int32]string

fn main() {
    var m table
    var k int64 = 1
    _ = m[k]
}
"#,
            "HYB-E1002",
        ),
        (
            r#"
type table = map[int32]string

fn main() {
    var m table
    var k int64 = 1
    delete(m, k)
}
"#,
            "HYB-E2005",
        ),
    ] {
        let errs = check_errors(src);
        assert_eq!(errs.len(), 1, "expected exactly 1 error for:\n{src}\ngot: {errs:?}");
        assert_eq!(errs[0].0, expected_code, "unexpected code for:\n{src}\ngot: {errs:?}");
    }
}

#[test]
fn accepts_valid_inferred_maps_and_shadowed_inner_map_bindings() {
    let src = r#"
type table = map[int32]string

struct holder {
    m map[int32]string
}

fn make_table() map[int32]string {
    return map[int32]string{1: "a"}
}

fn main() {
    var k32 int32 = 1
    var r rune = 'a'
    m1 := map[int32]string{1: "a"}
    _ = m1[k32]
    _ = m1[r]
    _ = m1[1]
    delete(m1, k32)

    m2 := map[int32]string{}
    m2[k32] = "ok"
    delete(m2, r)

    h := holder{ m: m1 }
    _ = h.m[k32]
    delete(h.m, k32)

    _ = make_table()[k32]
    var t table = m1
    _ = t[k32]
    delete(t, k32)

    // Inner-scope shadowing of `m1` with a `map[int64]string` must not inherit
    // the outer `int32` key width.
    if true {
        m1 := map[int64]string{1: "inner"}
        var k64 int64 = 1
        _ = m1[k64]
        delete(m1, k64)
    }
}
"#;
    let ast = parse_file("test.hy", src).expect("test snippet must parse");
    let res = check_file(&ast);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "expected Accept, got diagnostics: {:?}",
        res.diagnostics
    );
}
