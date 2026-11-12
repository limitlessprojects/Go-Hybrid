//! issue_2491_append_spread_slice_element_soundness.rs
//!
//! Regression test for issue #2491:
//! `Soundness: append(s, t...) accepts mismatched slice element types with variadic spread (HYB-E1002)`
//!
//! SPEC-LANGUAGE §3.1 & GUARANTEES.md Row 13 (BUILDABLE).
//! In Go, variadic append `append(s, t...)` requires that `t` is a slice whose
//! element type matches `s`'s element type (with the special case of appending
//! a string to `[]byte`).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("snippet must parse");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn append_spread_with_mismatched_slice_elements_rejects_e1002() {
    let cases = [
        (
            "[]int and []string",
            r#"package main
fn main() {
    a := []int{1, 2}
    b := []string{"x", "y"}
    _ = append(a, b...)
}
"#,
        ),
        (
            "[]int and []float64",
            r#"package main
fn main() {
    a := []int{1, 2}
    b := []float64{1.5, 2.5}
    _ = append(a, b...)
}
"#,
        ),
        (
            "[]float64 and []int (catches competitor Float/Int kinds_conflict bypass)",
            r#"package main
fn main() {
    a := []float64{1.0, 2.0}
    b := []int{1, 2}
    _ = append(a, b...)
}
"#,
        ),
        (
            "[]int and []bool",
            r#"package main
fn main() {
    a := []int{1, 2}
    b := []bool{true, false}
    _ = append(a, b...)
}
"#,
        ),
        (
            "[]string and []int",
            r#"package main
fn main() {
    a := []string{"a"}
    b := []int{1, 2}
    _ = append(a, b...)
}
"#,
        ),
        (
            "[]float64 and string spread",
            r#"package main
fn main() {
    a := []float64{1.0}
    _ = append(a, "hello"...)
}
"#,
        ),
        (
            "[]int and array spread",
            r#"package main
fn main() {
    a := []int{1, 2}
    var b [2]int
    _ = append(a, b...)
}
"#,
        ),
    ];

    for (label, src) in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected reject for: {label}");
        let codes = error_codes(&r);
        assert!(
            codes.contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for {label}, got diagnostics: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn valid_append_spread_and_single_elements_accept() {
    let src = r#"package main

fn main() {
    // Matching slice element types
    a := []int{1, 2}
    b := []int{3, 4}
    _ = append(a, b...)

    f1 := []float64{1.1, 2.2}
    f2 := []float64{3.3, 4.4}
    _ = append(f1, f2...)

    s1 := []string{"hello"}
    s2 := []string{"world"}
    _ = append(s1, s2...)

    // Valid string append to []byte
    bs := []byte("init")
    _ = append(bs, "extra"...)

    // Valid single element appends
    _ = append(a, 10, 20)
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept, got diagnostics: {:?}",
        r.diagnostics
    );
}
