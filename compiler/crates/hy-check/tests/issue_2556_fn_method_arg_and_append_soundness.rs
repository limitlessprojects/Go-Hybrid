//! issue_2556_fn_method_arg_and_append_soundness.rs — tests for HYB-E1002 rejection
//! of typed integer variables passed to float64 function parameters,
//! method parameters, and float slice append operations (fixes #2556).

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
fn typed_int_var_passed_to_float_fn_rejects_e1002() {
    let src = "\
fn take_float(f float64) {
    _ = f
}

fn main() {
    var i int = 1
    take_float(i)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn typed_int_var_passed_to_float_method_rejects_e1002() {
    let src = "\
struct worker {}

fn (w worker) work(f float64) {
    _ = f
}

fn main() {
    var w worker
    var i int = 1
    w.work(i)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn typed_int_var_appended_to_float_slice_rejects_e1002() {
    let src = "\
fn main() {
    var s []float64
    var i int = 1
    s = append(s, i)
    _ = s
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn valid_untyped_int_constants_in_calls_and_append_accepted() {
    let src = "\
const file_k = 100

struct worker {}

fn take_float(f float64) {
    _ = f
}

fn (w worker) work(f float64) {
    _ = f
}

fn main() {
    const local_k = 50
    var w worker
    take_float(1)
    take_float(42)
    take_float(file_k)
    take_float(local_k)
    take_float(-5)
    take_float(2 + 3)
    w.work(1)
    w.work(100)
    w.work(file_k)
    w.work(local_k)

    var s []float64
    s = append(s, 1)
    s = append(s, 2, 3)
    s = append(s, file_k)
    s = append(s, local_k)
    _ = s
}
";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept for untyped int constants, got diags: {:?}",
        r.diagnostics
    );
}
