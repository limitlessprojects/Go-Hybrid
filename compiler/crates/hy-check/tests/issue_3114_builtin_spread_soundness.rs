//! issue_3114_builtin_spread_soundness.rs — tests for HYB-E1002 rejection of
//! variadic spread `...` in calls to non-append built-in functions
//! (panic, print, println, real, imag, complex).
//!
//! Under Go language rules (§Calls, §Built-in functions) and Hybrid's Row 13
//! BUILDABLE guarantee, only `append` accepts a spread argument `...`.
//! Passing `...` to panic, print, println, real, imag, or complex is rejected
//! at compile time by Go (`invalid use of ... in call to ...`).

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
fn panic_with_spread_rejects_e1002() {
    let src = "\
fn main() {
    xs := []string{\"fatal error\"}
    panic(xs...)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn println_with_spread_rejects_e1002() {
    let src = "\
fn main() {
    nums := []int{1, 2, 3}
    println(nums...)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn print_with_spread_rejects_e1002() {
    let src = "\
fn main() {
    words := []string{\"hello\", \"world\"}
    print(words...)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn real_with_spread_rejects_e1002() {
    let src = "\
fn main() {
    c := complex(1.0, 2.0)
    _ = c
    c_vals := []complex128{c}
    _ = real(c_vals...)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn imag_with_spread_rejects_e1002() {
    let src = "\
fn main() {
    c := complex(1.0, 2.0)
    _ = c
    c_vals := []complex128{c}
    _ = imag(c_vals...)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn complex_with_spread_rejects_e1002() {
    let src = "\
fn main() {
    f_vals := []float64{1.0, 2.0}
    _ = complex(f_vals...)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn valid_append_spread_and_regular_builtins_accepted() {
    let src = "\
fn main() {
    xs := []int{1, 2}
    ys := []int{3, 4}
    _ = append(xs, ys...)
    println(\"ok\")
    print(\"status\")
    _ = real(complex(1.0, 2.0))
    _ = imag(complex(1.0, 2.0))
    _ = complex(1.0, 2.0)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
}
