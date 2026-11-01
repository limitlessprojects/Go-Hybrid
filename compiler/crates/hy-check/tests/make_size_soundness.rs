//! make_size_soundness.rs — tests for HYB-E1002 rejection of non-integer
//! size arguments (len, cap, channel buffer size) in built-in `make`.

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
fn make_slice_len_float_rejects_e1002() {
    let src = "fn main() {\n    f := 10.5\n    xs := make([]int, f)\n    _ = xs\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn make_slice_len_string_rejects_e1002() {
    let src = "fn main() {\n    s := \"hello\"\n    xs := make([]int, s)\n    _ = xs\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn make_slice_cap_float_rejects_e1002() {
    let src = "fn main() {\n    f := 20.0\n    xs := make([]int, 5, f)\n    _ = xs\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn make_chan_buffer_float_rejects_e1002() {
    let src = "fn main() {\n    f := 3.14\n    ch := make(chan int, f)\n    _ = ch\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn make_chan_buffer_string_rejects_e1002() {
    let src = "fn main() {\n    s := \"bad\"\n    ch := make(chan int, s)\n    _ = ch\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn make_valid_integer_variables_accepted() {
    let src = "fn main() {\n    n := 10\n    c := 20\n    xs := make([]int, n, c)\n    ch := make(chan int, n)\n    _ = xs\n    _ = ch\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty());
}

#[test]
fn make_valid_integer_literals_accepted() {
    let src = "fn main() {\n    xs := make([]int, 5, 10)\n    ch := make(chan int, 2)\n    _ = xs\n    _ = ch\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty());
}
