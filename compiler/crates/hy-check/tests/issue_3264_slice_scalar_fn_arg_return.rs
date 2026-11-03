//! issue_3264_slice_scalar_fn_arg_return.rs
//!
//! Regression test for issue #3264:
//! Function/method call arguments and return statements must enforce slice element
//! scalar width soundness and array dimension soundness under HYB-E1002.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn fn_arg_slice_scalar_width_mismatch_rejects_e1002() {
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
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for fn arg slice scalar mismatch, got: {codes:?}"
    );
}

#[test]
fn method_arg_slice_scalar_width_mismatch_rejects_e1002() {
    let src = "\
struct processor {}

fn (p *processor) process(items []int64) {
    _ = items
}

fn main() {
    var p processor
    var b []int32
    p.process(b)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for method arg slice scalar mismatch, got: {codes:?}"
    );
}

#[test]
fn return_slice_scalar_width_mismatch_rejects_e1002() {
    let src = "\
fn return_slice() []int64 {
    var b []int32
    return b
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for return slice scalar mismatch, got: {codes:?}"
    );
}

#[test]
fn return_array_dimension_mismatch_rejects_e1002() {
    let src = "\
fn return_arr() [3]int {
    var b [4]int
    return b
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for return array dimension mismatch, got: {codes:?}"
    );
}

#[test]
fn aliased_slice_arg_scalar_mismatch_rejects_e1002() {
    let src = "\
type my_i64s = []int64
type my_i32s = []int32

fn consume(s my_i64s) {
    _ = s
}

fn main() {
    var b my_i32s
    consume(b)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for aliased slice arg scalar mismatch, got: {codes:?}"
    );
}

#[test]
fn aliased_slice_return_scalar_mismatch_rejects_e1002() {
    let src = "\
type my_i64s = []int64
type my_i32s = []int32

fn produce() my_i64s {
    var b my_i32s
    return b
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for aliased slice return scalar mismatch, got: {codes:?}"
    );
}

#[test]
fn matching_slice_args_and_returns_accept() {
    let src = "\
struct processor {}

fn (p *processor) process(items []int64) []int64 {
    return items
}

fn take_slice(s []int64) [3]int {
    var arr [3]int
    return arr
}

fn main() {
    var p processor
    var b []int64
    res := p.process(b)
    _ = res
    arr := take_slice(b)
    _ = arr
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    let codes = error_codes(&r);
    assert!(
        codes.is_empty(),
        "expected accept for matching slice args and returns, got: {codes:?}"
    );
}
