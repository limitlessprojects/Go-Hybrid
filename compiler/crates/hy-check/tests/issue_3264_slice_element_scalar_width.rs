//! issue_3264_slice_element_scalar_width.rs — tests for HYB-E1002 rejection
//! of mismatched slice element scalar widths in call arguments and return
//! statements (fixes #3264).

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
fn issue_3264_reproduction_snippet_rejects_e1002() {
    let src = "\
fn take_slice(s []int64) {
    _ = s
}

fn return_slice() []int64 {
    var b []int32
    return b
}

fn main() {
    var b []int32
    take_slice(b)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(codes.contains(&"HYB-E1002".to_string()));
    assert_eq!(
        codes.iter().filter(|c| *c == "HYB-E1002").count(),
        2,
        "expected 2 HYB-E1002 errors: one in return_slice, one in main"
    );
}

#[test]
fn typed_slice_arg_to_fn_mismatched_element_width_rejects_e1002() {
    let src = "\
fn take_slice(s []int64) {
    _ = s
}

fn main() {
    var b []int32
    take_slice(b)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn typed_slice_return_mismatched_element_width_rejects_e1002() {
    let src = "\
fn return_slice() []int64 {
    var b []int32
    return b
}

fn main() {
    _ = return_slice()
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn typed_slice_method_arg_mismatched_element_width_rejects_e1002() {
    let src = "\
struct processor {}

fn (p processor) process(data []float64) {
    _ = data
}

fn main() {
    var p processor
    var s []float32
    p.process(s)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn slice_literal_arg_mismatched_element_width_rejects_e1002() {
    let src = "\
fn take_slice(s []int64) {
    _ = s
}

fn main() {
    take_slice([]int32{1, 2, 3})
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn matching_slice_element_scalar_width_accepted() {
    let src = "\
struct processor {}

fn (p processor) process(data []float64) {
    _ = data
}

fn take_slice(s []int64) {
    _ = s
}

fn return_slice() []int64 {
    var b []int64
    return b
}

fn main() {
    var b []int64
    take_slice(b)
    take_slice([]int64{1, 2, 3})

    var p processor
    var data []float64
    p.process(data)

    _ = return_slice()
}
";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept for matching slice elements, got: {:?}",
        r.diagnostics
    );
}
