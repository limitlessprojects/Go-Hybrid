//! nonvariadic_spread_builtins_panic_make.rs - issue #4012 (Row 13 BUILDABLE).
//!
//! Variadic spread `...` is only permitted on the variadic built-in `append`.
//! Built-ins `panic`, `print`, `println`, `make`, `new`, `complex`, `real`, `imag`,
//! and `recover` do not take variadic arguments. Passing `...` to them emits unbuildable
//! Go code (`invalid use of ... with built-in <name>`). `hy-check` must reject with `HYB-E1002`.

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
fn spread_on_builtin_panic_rejects_e1002() {
    let src = r#"
fn main() {
    msg := "boom"
    panic(msg...)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for panic(msg...), got {:?}",
        r.diagnostics
    );
}

#[test]
fn spread_on_builtin_print_and_println_rejects_e1002() {
    let src = r#"
fn main() {
    xs := []int{1, 2}
    print(xs...)
    println(xs...)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for print/println spread, got {:?}",
        r.diagnostics
    );
}

#[test]
fn spread_on_builtin_make_and_new_rejects_e1002() {
    let src = r#"
fn main() {
    n := []int{10}
    _ = make([]int, n...)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for make spread, got {:?}",
        r.diagnostics
    );
}

#[test]
fn append_spread_still_accepted() {
    let src = r#"
fn main() {
    a := []int{1}
    b := []int{2}
    c := append(a, b...)
    _ = c
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(
        error_codes(&r).is_empty(),
        "expected 0 errors, got {:?}",
        r.diagnostics
    );
}
