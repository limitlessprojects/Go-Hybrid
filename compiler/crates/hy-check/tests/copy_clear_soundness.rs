//! copy_clear_soundness.rs — tests for HYB-E1002 / HYB-E2005 rejection of invalid
//! copy and clear builtin operands, element types, and return values.

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

#[test]
fn copy_element_types_mismatch_rejects_e1002() {
    let src = "\
fn main() {
    dst := []int{1, 2}
    src := []string{\"a\", \"b\"}
    _ = copy(dst, src)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn copy_dst_non_slice_rejects_e2005() {
    let src = "\
fn main() {
    x := 10
    s := []int{1, 2}
    _ = copy(x, s)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E2005".to_string()));
}

#[test]
fn copy_src_non_slice_rejects_e2005() {
    let src = "\
fn main() {
    s := []int{1, 2}
    x := 10
    _ = copy(s, x)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E2005".to_string()));
}

#[test]
fn copy_string_to_non_byte_slice_rejects_e1002() {
    let src = "\
fn main() {
    dst := []string{\"a\", \"b\"}
    _ = copy(dst, \"hello\")
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn copy_string_to_int_slice_rejects_e1002() {
    let src = "\
fn main() {
    var dst []int = []int{1, 2, 3, 4}
    var src string = \"test\"
    copy(dst, src)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn copy_string_to_rune_slice_rejects_e1002() {
    let src = "\
fn main() {
    dst := []rune{1, 2}
    copy(dst, \"test\")
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn copy_return_type_mismatch_rejects_e1002() {
    let src = "\
fn f(dst []int, src []int) string {
    return copy(dst, src)
}
fn main() {
    _ = f([]int{1}, []int{2})
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn copy_assign_type_mismatch_rejects_e1002() {
    let src = "\
fn main() {
    dst := []int{1, 2}
    src := []int{3, 4}
    s := \"hello\"
    s = copy(dst, src)
    _ = s
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn clear_non_slice_non_map_rejects_e2005() {
    let src = "\
fn main() {
    x := 10
    clear(x)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E2005".to_string()));
}

#[test]
fn clear_used_as_value_rejects_e1002() {
    let src = "\
fn main() {
    s := []int{1, 2}
    x := clear(s)
    _ = x
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn panic_used_as_value_rejects_e1002() {
    let src = "\
fn main() {
    x := panic(\"err\")
    _ = x
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn valid_copy_and_clear_accepted() {
    let src = "\
fn main() {
    dst := []int{1, 2}
    src := []int{3, 4}
    n := copy(dst, src)
    _ = n

    b_dst := []byte{0, 0, 0}
    n2 := copy(b_dst, \"abc\")
    _ = n2

    s := []int{1, 2}
    clear(s)

    m := map[string]int{\"a\": 1}
    clear(m)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(error_codes(&r).is_empty(), "expected 0 errors, got {:?}", r.diagnostics);
}
