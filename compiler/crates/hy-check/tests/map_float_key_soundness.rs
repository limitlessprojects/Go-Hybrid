//! #2639: typed `int` variables previously escaped type-checking when indexing
//! or deleting from maps with floating-point keys (`map[float64]T` or `map[float32]T`).
//! In Go, passing a typed integer variable as a floating-point map key fails
//! `go build` with "cannot use i (variable of type int) as float64 value in map index".
//!
//! Untyped integer constants (`m[1]`, `delete(m, 1)`) remain legal per Go's
//! constant assignability rules.

use hy_check::{check_file, CheckResult};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn errors(r: &CheckResult, code: &str) -> usize {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error && d.code == code)
        .count()
}

#[test]
fn issue_2639_map_float_index_typed_int_rejected() {
    let src = r#"
fn main() {
    var m map[float64]string
    var i int = 1
    _ = m[i]
}
"#;
    let r = check(src);
    assert_eq!(errors(&r, "HYB-E1002"), 1, "{:?}", r.diagnostics);
}

#[test]
fn issue_2639_map_float_assignment_index_typed_int_rejected() {
    let src = r#"
fn main() {
    var m map[float64]string
    var i int = 1
    m[i] = "hello"
}
"#;
    let r = check(src);
    assert_eq!(errors(&r, "HYB-E1002"), 1, "{:?}", r.diagnostics);
}

#[test]
fn issue_2639_map_float_delete_typed_int_rejected() {
    let src = r#"
fn main() {
    var m map[float64]string
    var i int = 1
    delete(m, i)
}
"#;
    let r = check(src);
    assert_eq!(errors(&r, "HYB-E2005"), 1, "{:?}", r.diagnostics);
}

#[test]
fn issue_2639_untyped_constants_and_matching_types_accepted() {
    let src = r#"
fn main() {
    var m map[float64]string
    var f float64 = 1.5
    _ = m[1]
    _ = m[1.5]
    _ = m[f]
    m[1] = "ok"
    m[1.5] = "ok"
    m[f] = "ok"
    delete(m, 1)
    delete(m, 1.5)
    delete(m, f)
}
"#;
    let r = check(src);
    assert_eq!(errors(&r, "HYB-E1002"), 0, "{:?}", r.diagnostics);
    assert_eq!(errors(&r, "HYB-E2005"), 0, "{:?}", r.diagnostics);
}