//! Regression tests for Issue #2476:
//! SPEC-LANGUAGE §3.1 LX13 / GUARANTEES.md Row 13 (BUILDABLE)
//! Range loops `for i in lo..hi` and `for i in lo..=hi` must strictly enforce
//! integer range bounds. Inferred non-integer bounds (such as float, bool, string,
//! or binary arithmetic expressions over floats) must be rejected with `HYB-E2009`
//! instead of escaping checker validation and failing with type errors in `go build`.

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let f = parse_file("test.hy", src).expect("parse ok");
    check_file(&f)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn inferred_float_and_bool_range_bounds_reject_e2009() {
    let cases = [
        ("float literal binding as hi", "f := 3.14\n    for i in 0..f { _ = i }"),
        ("float literal binding as lo", "f := 1.5\n    for i in f..10 { _ = i }"),
        ("bool literal binding as hi", "b := true\n    for i in 0..b { _ = i }"),
        ("bool literal binding as lo", "b := false\n    for i in b..5 { _ = i }"),
        ("string literal binding as hi", "s := \"hello\"\n    for i in 0..s { _ = i }"),
        ("float binary addition", "f1 := 1.5\n    f2 := 2.5\n    for i in 0..(f1 + f2) { _ = i }"),
        ("inclusive range with inferred float", "f := 5.5\n    for i in 0..=f { _ = i }"),
        ("comparison bool binary expr in range", "a := 1\n    b := 2\n    for i in 0..(a < b) { _ = i }"),
    ];

    for (label, body) in cases {
        let src = format!(
            r#"
fn main() {{
    {body}
}}
"#
        );
        let c = codes(&src);
        assert!(
            c.contains(&"HYB-E2009".to_string()),
            "expected HYB-E2009 for {label}: `{body}`, got diagnostics: {c:?}"
        );
    }
}

#[test]
fn valid_inferred_and_typed_integer_ranges_stay_accepted() {
    let src = r#"
fn main() {
    n := 10
    for i in 0..n {
        _ = i
    }

    start := 1
    end := 5
    for i in start..end {
        _ = i
    }

    for i in 0..=n {
        _ = i
    }

    var a int = 2
    var b int = 8
    for i in a..b {
        _ = i
    }
}
"#;
    let c = codes(src);
    assert!(
        c.is_empty(),
        "expected 0 diagnostics for valid integer ranges, got: {c:?}"
    );
}
