//! #3230: `set[T]` methods (`insert`, `contains`, `remove`) and `delete(s, v)`
//! must check concrete numeric scalar widths (`scalar_names_conflict`) and
//! fixed-size array lengths (`sized_array_names_differ`).
//!
//! Prior to #3230, `check_set_method_arg` and `delete(s, v)` only checked
//! `kinds_conflict(&want, &got) || int_to_float_conflict`. Under this broad
//! `OpKind` check, passing `int` to `set[int64]` or `[3]int` to `set[[2]int]`
//! passed `hyb check` with 0 diagnostics, and failed `go build` with
//! `cannot use ... as ... value in map index` (Row 13 BUILDABLE).

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
fn test_set_insert_scalar_mismatch_rejected() {
    let src = r#"
fn main() {
    var s set[int64]
    var x int = 1
    s.insert(x)
}
"#;
    let r = check(src);
    assert_eq!(count_errors(&r, "HYB-E1002"), 1, "{:?}", r.diagnostics);
}

#[test]
fn test_set_insert_float_scalar_mismatch_rejected() {
    let src = r#"
fn main() {
    var s set[float64]
    var f float32 = 1.5
    s.insert(f)
}
"#;
    let r = check(src);
    assert_eq!(count_errors(&r, "HYB-E1002"), 1, "{:?}", r.diagnostics);
}

#[test]
fn test_set_insert_uint_scalar_mismatch_rejected() {
    let src = r#"
fn main() {
    var s set[uint64]
    var u uint32 = 10
    s.insert(u)
}
"#;
    let r = check(src);
    assert_eq!(count_errors(&r, "HYB-E1002"), 1, "{:?}", r.diagnostics);
}

#[test]
fn test_set_insert_array_length_mismatch_rejected() {
    let src = r#"
fn main() {
    var s set[[2]int]
    var a [3]int
    s.insert(a)
}
"#;
    let r = check(src);
    assert_eq!(count_errors(&r, "HYB-E1002"), 1, "{:?}", r.diagnostics);

    let src_lit = r#"
fn main() {
    var s set[[2]int]
    s.insert([3]int{1, 2, 3})
}
"#;
    let r_lit = check(src_lit);
    assert_eq!(count_errors(&r_lit, "HYB-E1002"), 1, "{:?}", r_lit.diagnostics);
}

#[test]
fn test_set_contains_scalar_and_array_mismatch_rejected() {
    let src1 = r#"
fn main() {
    var s set[int64]
    var x int = 1
    _ = s.contains(x)
}
"#;
    let r1 = check(src1);
    assert_eq!(count_errors(&r1, "HYB-E1002"), 1, "{:?}", r1.diagnostics);

    let src2 = r#"
fn main() {
    var s set[[2]int]
    var a [3]int
    _ = s.contains(a)
}
"#;
    let r2 = check(src2);
    assert_eq!(count_errors(&r2, "HYB-E1002"), 1, "{:?}", r2.diagnostics);
}

#[test]
fn test_set_remove_scalar_and_array_mismatch_rejected() {
    let src1 = r#"
fn main() {
    var s set[int64]
    var x int = 1
    s.remove(x)
}
"#;
    let r1 = check(src1);
    assert_eq!(count_errors(&r1, "HYB-E1002"), 1, "{:?}", r1.diagnostics);

    let src2 = r#"
fn main() {
    var s set[[2]int]
    var a [3]int
    s.remove(a)
}
"#;
    let r2 = check(src2);
    assert_eq!(count_errors(&r2, "HYB-E1002"), 1, "{:?}", r2.diagnostics);
}

#[test]
fn test_delete_set_scalar_and_array_mismatch_rejected() {
    let src1 = r#"
fn main() {
    var s set[int64]
    var x int = 1
    delete(s, x)
}
"#;
    let r1 = check(src1);
    assert_eq!(count_errors(&r1, "HYB-E2005"), 1, "{:?}", r1.diagnostics);

    let src2 = r#"
fn main() {
    var s set[[2]int]
    var a [3]int
    delete(s, a)
}
"#;
    let r2 = check(src2);
    assert_eq!(count_errors(&r2, "HYB-E2005"), 1, "{:?}", r2.diagnostics);

    let src3 = r#"
fn main() {
    var s set[[2]int]
    delete(s, [3]int{1, 2, 3})
}
"#;
    let r3 = check(src3);
    assert_eq!(count_errors(&r3, "HYB-E2005"), 1, "{:?}", r3.diagnostics);
}

#[test]
fn test_set_valid_operations_accepted() {
    let src = r#"
fn main() {
    var s1 set[int64]
    var x int64 = 1
    s1.insert(x)
    _ = s1.contains(x)
    s1.remove(x)
    delete(s1, x)

    var s2 set[[2]int]
    var a [2]int
    s2.insert(a)
    _ = s2.contains(a)
    s2.remove(a)
    delete(s2, a)

    var s3 set[int]
    s3.insert(42)

    var s4 set[float64]
    s4.insert(3.14)

    var s5 set[int64]
    var y int = 10
    s5.insert(int64(y))
    delete(s5, int64(y))
}
"#;
    let r = check(src);
    assert_eq!(count_errors(&r, "HYB-E1002"), 0, "{:?}", r.diagnostics);
    assert_eq!(count_errors(&r, "HYB-E2005"), 0, "{:?}", r.diagnostics);
}
