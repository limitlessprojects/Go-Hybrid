//! #2921 — set[T] results and elements are untyped:
//! 1. `var b string = s.insert(1)` — `insert`/`contains` produce `bool`, reject mismatch.
//! 2. `for v in s { var t string = v }` — `v` bound as element type `T`, reject mismatch.
//! 3. `ss := []set[int]{set[int]{}}; ss[0].insert("x")` — argument check runs on indexed/field sets.
//! 4. `m := map[string]set[int]{}; m["a"].contains("z")` — unaddressable receiver rejected (HYB-E1002).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_reject(src: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got: {:?}",
        r.diagnostics
    );
}

fn assert_accept(src: &str) {
    let r = check(src);
    assert!(
        error_codes(&r).is_empty(),
        "expected accept, got diagnostics: {:?}",
        r.diagnostics
    );
}

#[test]
fn test_repro_1_set_insert_result_is_bool() {
    assert_reject(
        r#"fn main() {
    s := set[int]{}
    var b string = s.insert(1)
    fmt.println(b)
}
"#,
    );
}

#[test]
fn test_repro_1_set_contains_result_is_bool() {
    assert_reject(
        r#"fn main() {
    s := set[int]{}
    var b string = s.contains(1)
    fmt.println(b)
}
"#,
    );
}

#[test]
fn test_repro_2_set_for_in_element_binding_typed() {
    assert_reject(
        r#"fn main() {
    s := set[int]{}
    s.insert(1)
    for v in s {
        var t string = v
        fmt.println(t)
    }
}
"#,
    );
}

#[test]
fn test_repro_3_indexed_set_method_arg_type_checked() {
    assert_reject(
        r#"fn main() {
    ss := []set[int]{set[int]{}}
    ss[0].insert("x")
}
"#,
    );
}

#[test]
fn test_repro_4_map_indexed_set_pointer_method_unaddressable() {
    assert_reject(
        r#"fn main() {
    m := map[string]set[int]{}
    fmt.println(m["a"].contains(1))
}
"#,
    );
}

#[test]
fn test_controls_valid_set_usage_accepted() {
    assert_accept(
        r#"fn main() {
    s := set[int]{}
    var b bool = s.insert(1)
    fmt.println(b)
    var c bool = s.contains(1)
    fmt.println(c)
    for v in s {
        var n int = v
        fmt.println(n)
    }
    ss := []set[int]{set[int]{}}
    ss[0].insert(42)
}
"#,
    );
}
