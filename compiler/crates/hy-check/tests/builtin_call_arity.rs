//! builtin_call_arity.rs — tests for HYB-E2005 rejection of invalid arities
//! on predeclared Go built-in calls (panic, recover, min, max, real, imag, complex).

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
fn panic_zero_args_rejects_e2005() {
    let src = "fn main() {\n    panic()\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E2005".to_string()));
}

#[test]
fn panic_too_many_args_rejects_e2005() {
    let src = "fn main() {\n    panic(1, 2)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E2005".to_string()));
}

#[test]
fn recover_with_args_rejects_e2005() {
    let src = "fn main() {\n    recover(1)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E2005".to_string()));
}

#[test]
fn min_zero_args_rejects_e2005() {
    let src = "fn main() {\n    x := min()\n    _ = x\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E2005".to_string()));
}

#[test]
fn max_zero_args_rejects_e2005() {
    let src = "fn main() {\n    x := max()\n    _ = x\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E2005".to_string()));
}

#[test]
fn complex_bad_arity_rejects_e2005() {
    let r0 = check("fn main() {\n    c := complex()\n    _ = c\n}\n");
    assert_eq!(r0.verdict, Verdict::Reject);
    assert!(error_codes(&r0).contains(&"HYB-E2005".to_string()));

    let r1 = check("fn main() {\n    c := complex(1)\n    _ = c\n}\n");
    assert_eq!(r1.verdict, Verdict::Reject);
    assert!(error_codes(&r1).contains(&"HYB-E2005".to_string()));

    let r3 = check("fn main() {\n    c := complex(1, 2, 3)\n    _ = c\n}\n");
    assert_eq!(r3.verdict, Verdict::Reject);
    assert!(error_codes(&r3).contains(&"HYB-E2005".to_string()));
}

#[test]
fn real_and_imag_bad_arity_rejects_e2005() {
    let r_real = check("fn main() {\n    r := real()\n    _ = r\n}\n");
    assert_eq!(r_real.verdict, Verdict::Reject);
    assert!(error_codes(&r_real).contains(&"HYB-E2005".to_string()));

    let r_imag = check("fn main() {\n    i := imag(1, 2)\n    _ = i\n}\n");
    assert_eq!(r_imag.verdict, Verdict::Reject);
    assert!(error_codes(&r_imag).contains(&"HYB-E2005".to_string()));
}

#[test]
fn closure_panic_bad_arity_rejects_e2005() {
    let src = "fn main() {\n    f := fn() {\n        panic()\n    }\n    _ = f\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E2005".to_string()));
}

#[test]
fn valid_builtins_accepted() {
    let src = "\
fn main() {
    panic(\"error\")
    r := recover()
    m1 := min(1)
    m2 := min(1, 2)
    m3 := max(1, 2)
    c := complex(1.0, 2.0)
    re := real(c)
    im := imag(c)
    _ = r
    _ = m1
    _ = m2
    _ = m3
    _ = c
    _ = re
    _ = im
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(error_codes(&r).is_empty(), "expected 0 errors, got {:?}", r.diagnostics);
}
