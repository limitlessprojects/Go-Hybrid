//! issue_3114_builtin_spread_rejections.rs — tests for HYB-E1002 rejection
//! of spread arguments (...) on non-variadic built-ins panic, print, println,
//! real, imag, complex, make, new, recover (fixes #3114, Row 13 BUILDABLE).

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
fn spread_on_panic_rejects_e1002() {
    let src = "\
fn main() {
    xs := []string{\"failure\"}
    panic(xs...)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn spread_on_print_and_println_rejects_e1002() {
    let cases = [
        "fn main() {\n    nums := []int{1, 2}\n    println(nums...)\n}\n",
        "fn main() {\n    nums := []int{1, 2}\n    print(nums...)\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected rejection for:\n{}",
            src
        );
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for:\n{}",
            src
        );
    }
}

#[test]
fn spread_on_real_and_imag_rejects_e1002() {
    let cases = [
        "fn main() {\n    c := complex(1.0, 2.0)\n    xs := []complex128{c}\n    _ = real(xs...)\n}\n",
        "fn main() {\n    c := complex(1.0, 2.0)\n    xs := []complex128{c}\n    _ = imag(xs...)\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected rejection for:\n{}",
            src
        );
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for:\n{}",
            src
        );
    }
}

#[test]
fn spread_on_complex_rejects_e1002() {
    let src = "\
fn main() {
    f_vals := []float64{1.0, 2.0}
    _ = complex(f_vals...)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for:\n{}",
        src
    );
}

#[test]
fn spread_on_make_new_recover_rejects_e1002() {
    let cases = [
        "fn main() {\n    xs := []int{1, 2}\n    _ = make([]int, xs...)\n}\n",
        "fn main() {\n    xs := []int{1, 2}\n    _ = new(int, xs...)\n}\n",
        "fn main() {\n    xs := []int{1, 2}\n    _ = recover(xs...)\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected rejection for:\n{}",
            src
        );
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for:\n{}",
            src
        );
    }
}

#[test]
fn non_spread_builtins_and_append_spread_stay_accepted() {
    let src = "\
fn main() {
    nums := []int{1, 2}
    println(nums)
    print(1)
    c := complex(1.0, 2.0)
    _ = real(c)
    _ = imag(c)
    _ = complex(1.0, 2.0)
    _ = make([]int, 2)
    _ = new(int)
    a := []int{1}
    b := []int{2}
    c_slice := append(a, b...)
    _ = c_slice
}
";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected clean acceptance, got {:?}",
        r.diagnostics
    );
    assert!(
        r.diagnostics.is_empty(),
        "expected 0 diagnostics, got {:?}",
        r.diagnostics
    );
}
