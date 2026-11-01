//! nonvariadic_spread_2701.rs — issue #2701 (Row 13 BUILDABLE).
//!
//! The variadic spread `xs...` is only legal on the variadic built-in `append`.
//! Hybrid declares no variadic parameters (`parse_params` never accepts `...T`),
//! so passing `...` to a non-variadic user function, method, or built-in emits
//! Go that fails `go build` (`cannot use ... in call to non-variadic f`,
//! `invalid use of ... with built-in len`). `hy-check` must reject it with
//! `HYB-E1002`.

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
fn spread_on_non_variadic_user_fn_rejects_e1002() {
    let src = "\
fn foo(s []int) {}
fn main() {
    xs := []int{1, 2}
    foo(xs...)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "{:?}",
        r.diagnostics
    );
}

#[test]
fn spread_on_non_variadic_method_rejects_e1002() {
    let src = "\
struct box {
    v int
}
fn (b box) take(s []int) {
}
fn main() {
    b := box{v: 0}
    xs := []int{1}
    b.take(xs...)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "{:?}",
        r.diagnostics
    );
}

#[test]
fn spread_on_builtin_len_rejects_e1002() {
    let src = "\
fn main() {
    xs := []int{1, 2}
    _ = len(xs...)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "{:?}",
        r.diagnostics
    );
}

#[test]
fn spread_on_builtin_cap_rejects_e1002() {
    let src = "\
fn main() {
    xs := []int{1, 2}
    _ = cap(xs...)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "{:?}",
        r.diagnostics
    );
}

#[test]
fn spread_on_builtin_close_rejects_e1002() {
    let src = "\
fn main() {
    ch := make(chan int, 1)
    close(ch...)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "{:?}",
        r.diagnostics
    );
}

#[test]
fn spread_on_builtin_copy_rejects_e1002() {
    let src = "\
fn main() {
    a := []int{1}
    b := []int{2}
    _ = copy(a, b...)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "{:?}",
        r.diagnostics
    );
}

#[test]
fn spread_on_builtin_min_rejects_e1002() {
    let src = "\
fn main() {
    xs := []int{1, 2}
    _ = min(xs...)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "{:?}",
        r.diagnostics
    );
}

#[test]
fn append_spread_is_still_accepted() {
    let src = "\
fn main() {
    a := []int{1}
    b := []int{2}
    c := append(a, b...)
    _ = c
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(
        error_codes(&r).is_empty(),
        "expected 0 errors, got {:?}",
        r.diagnostics
    );
}

#[test]
fn non_spread_calls_are_accepted() {
    let src = "\
fn foo(s []int) {}
fn main() {
    xs := []int{1, 2}
    foo(xs)
    _ = len(xs)
    n := copy(xs, xs)
    _ = n
    ch := make(chan int, 1)
    close(ch)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(
        error_codes(&r).is_empty(),
        "expected 0 errors, got {:?}",
        r.diagnostics
    );
}
