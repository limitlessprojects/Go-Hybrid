//! issue_2896_complex_real_imag_soundness.rs — regression tests for Issue #2896:
//! built-in complex, real, and imag operand type validation and scalar checks (HYB-E2005 / HYB-E1002).

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
fn test_real_imag_non_complex_rejected_e2005() {
    let cases = [
        ("real on int32", "fn test() {\n    var x int32 = 1\n    _ = real(x)\n}\n"),
        ("imag on int32", "fn test() {\n    var x int32 = 1\n    _ = imag(x)\n}\n"),
        ("real on float64", "fn test() {\n    var x float64 = 1.0\n    _ = real(x)\n}\n"),
        ("imag on float64", "fn test() {\n    var x float64 = 1.0\n    _ = imag(x)\n}\n"),
        ("real on string", "fn test() {\n    var s string = \"abc\"\n    _ = real(s)\n}\n"),
        ("imag on bool", "fn test() {\n    var b bool = true\n    _ = imag(b)\n}\n"),
        ("real on slice", "fn test() {\n    var s []float64\n    _ = real(s)\n}\n"),
    ];

    for (desc, src) in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for {}", desc);
        assert!(
            error_codes(&r).contains(&"HYB-E2005".to_string()),
            "expected HYB-E2005 for {}, got: {:?}",
            desc,
            r.diagnostics
        );
    }
}

#[test]
fn test_complex_non_float_rejected_e2005() {
    let cases = [
        ("complex with int variables", "fn test() {\n    var r int32 = 1\n    var i int32 = 2\n    _ = complex(r, i)\n}\n"),
        ("complex with real int var", "fn test() {\n    var r int32 = 1\n    _ = complex(r, 2.0)\n}\n"),
        ("complex with imag int var", "fn test() {\n    var i int32 = 2\n    _ = complex(1.0, i)\n}\n"),
        ("complex with string", "fn test() {\n    var s string = \"x\"\n    _ = complex(1.0, s)\n}\n"),
        ("complex with bool", "fn test() {\n    var b bool = true\n    _ = complex(b, 1.0)\n}\n"),
    ];

    for (desc, src) in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for {}", desc);
        assert!(
            error_codes(&r).contains(&"HYB-E2005".to_string()),
            "expected HYB-E2005 for {}, got: {:?}",
            desc,
            r.diagnostics
        );
    }
}

#[test]
fn test_complex_mismatched_float_widths_rejected_e1002() {
    let cases = [
        ("float32 real and float64 imag", "fn test() {\n    var r float32 = 1.0\n    var i float64 = 2.0\n    _ = complex(r, i)\n}\n"),
        ("float64 real and float32 imag", "fn test() {\n    var r float64 = 1.0\n    var i float32 = 2.0\n    _ = complex(r, i)\n}\n"),
        ("f32 real and f64 imag aliases", "fn test() {\n    var r f32 = 1.0\n    var i f64 = 2.0\n    _ = complex(r, i)\n}\n"),
    ];

    for (desc, src) in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for {}", desc);
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for {}, got: {:?}",
            desc,
            r.diagnostics
        );
    }
}

#[test]
fn test_complex_real_imag_variadic_spread_rejected_e1002() {
    let cases = [
        ("complex spread", "fn test(a []float64) {\n    _ = complex(a...)\n}\n"),
        ("real spread", "fn test(a []complex128) {\n    _ = real(a...)\n}\n"),
        ("imag spread", "fn test(a []complex128) {\n    _ = imag(a...)\n}\n"),
    ];

    for (desc, src) in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for {}", desc);
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for {}, got: {:?}",
            desc,
            r.diagnostics
        );
    }
}

#[test]
fn test_valid_complex_real_imag_accepted() {
    let cases = [
        ("float32 complex", "fn test() {\n    var r float32 = 1.0\n    var i float32 = 2.0\n    c := complex(r, i)\n    _ = c\n}\n"),
        ("float64 complex", "fn test() {\n    var r float64 = 1.0\n    var i float64 = 2.0\n    c := complex(r, i)\n    _ = c\n}\n"),
        ("float64 with untyped float const", "fn test() {\n    var r float64 = 1.0\n    c := complex(r, 2.0)\n    _ = c\n}\n"),
        ("float64 with untyped int const", "fn test() {\n    var r float64 = 1.0\n    c := complex(r, 2)\n    _ = c\n}\n"),
        ("untyped constants in complex", "fn test() {\n    c := complex(1.0, 2.0)\n    _ = c\n}\n"),
        ("real and imag on complex128", "fn test(c complex128) {\n    r := real(c)\n    im := imag(c)\n    _ = r\n    _ = im\n}\n"),
        ("real and imag on complex64", "fn test(c complex64) {\n    r := real(c)\n    im := imag(c)\n    _ = r\n    _ = im\n}\n"),
    ];

    for (desc, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "expected accept for {}, got diagnostics: {:?}",
            desc,
            r.diagnostics
        );
        assert!(
            error_codes(&r).is_empty(),
            "expected 0 errors for {}, got: {:?}",
            desc,
            r.diagnostics
        );
    }
}
