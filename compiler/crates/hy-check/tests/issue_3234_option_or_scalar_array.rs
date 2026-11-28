//! #3234: `option[T].or(fallback)` must enforce exact numeric scalar widths
//! (`scalar_names_conflict`) and fixed-size array lengths (`sized_array_names_differ`).
//!
//! Prior to #3234, `check_option_method_arg` only checked `kinds_conflict(&want, &got) || int_to_float_conflict`.
//! Under this coarse `OpKind` check, passing `int` to `option[int64].or()` or
//! `[3]int` to `option[[2]int].or()` passed `hyb check` with 0 diagnostics, and
//! failed downstream `go build` with `cannot use ... as ... value in argument to op.Or` (Row 13 BUILDABLE).

use hy_check::{check_file, CheckResult};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn count_errors(r: &CheckResult, code: &str) -> usize {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error && d.code == code)
        .count()
}

#[test]
fn test_option_or_int_scalar_mismatch_rejected() {
    let src = r#"
fn main() {
    var op option[int64]
    var d int = 10
    _ = op.or(d)
}
"#;
    let r = check(src);
    assert_eq!(count_errors(&r, "HYB-E1002"), 1, "{:?}", r.diagnostics);
}

#[test]
fn test_option_or_float_scalar_mismatch_rejected() {
    let src = r#"
fn main() {
    var op option[float64]
    var f float32 = 1.5
    _ = op.or(f)
}
"#;
    let r = check(src);
    assert_eq!(count_errors(&r, "HYB-E1002"), 1, "{:?}", r.diagnostics);
}

#[test]
fn test_option_or_uint_scalar_mismatch_rejected() {
    let src = r#"
fn main() {
    var op option[uint64]
    var u uint32 = 42
    _ = op.or(u)
}
"#;
    let r = check(src);
    assert_eq!(count_errors(&r, "HYB-E1002"), 1, "{:?}", r.diagnostics);
}

#[test]
fn test_option_or_array_length_mismatch_rejected() {
    let src = r#"
fn main() {
    var op option[[2]int]
    var a [3]int
    _ = op.or(a)
}
"#;
    let r = check(src);
    assert_eq!(count_errors(&r, "HYB-E1002"), 1, "{:?}", r.diagnostics);

    let src_lit = r#"
fn main() {
    var op option[[2]int]
    _ = op.or([3]int{1, 2, 3})
}
"#;
    let r_lit = check(src_lit);
    assert_eq!(count_errors(&r_lit, "HYB-E1002"), 1, "{:?}", r_lit.diagnostics);
}

#[test]
fn test_option_or_array_string_length_mismatch_rejected() {
    let src = r#"
fn main() {
    var op option[[4]string]
    var a [2]string
    _ = op.or(a)
}
"#;
    let r = check(src);
    assert_eq!(count_errors(&r, "HYB-E1002"), 1, "{:?}", r.diagnostics);
}

#[test]
fn test_option_or_valid_operations_accepted() {
    let src = r#"
fn main() {
    var op1 option[int64]
    var d1 int64 = 10
    _ = op1.or(d1)

    var op2 option[[2]int]
    var d2 [2]int
    _ = op2.or(d2)

    var op3 option[int]
    _ = op3.or(42)

    var op4 option[float64]
    _ = op4.or(3.14)

    var op5 option[int64]
    var x int = 5
    _ = op5.or(int64(x))

    var op6 option[[2]int]
    _ = op6.or([2]int{1, 2})
}
"#;
    let r = check(src);
    assert_eq!(count_errors(&r, "HYB-E1002"), 0, "{:?}", r.diagnostics);
}
