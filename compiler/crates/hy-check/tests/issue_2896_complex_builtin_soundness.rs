//! issue_2896_complex_builtin_soundness.rs
//! Regression tests for Issue #2896:
//! built-in complex, real, and imag allow invalid operand types and mismatched scalars.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("test source must parse");
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
fn rejects_invalid_operand_types_to_complex() {
    let cases = [
        // int32 variables passed to complex
        (
            "fn main() {\n    var r int32 = 1\n    var i int32 = 2\n    _ = complex(r, i)\n}\n",
            "HYB-E2005",
        ),
        // int and float64 mixed
        (
            "fn main() {\n    var r int = 1\n    var i float64 = 2.0\n    _ = complex(r, i)\n}\n",
            "HYB-E2005",
        ),
        // string passed to complex
        (
            "fn main() {\n    var s string = \"x\"\n    var i float64 = 2.0\n    _ = complex(s, i)\n}\n",
            "HYB-E2005",
        ),
        // bool passed to complex
        (
            "fn main() {\n    var b bool = true\n    var r float64 = 1.0\n    _ = complex(r, b)\n}\n",
            "HYB-E2005",
        ),
        // slice passed to complex
        (
            "fn main() {\n    var sl []float64\n    var i float64 = 2.0\n    _ = complex(sl, i)\n}\n",
            "HYB-E2005",
        ),
    ];

    for (src, expected_code) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected reject for:\n{src}\nDiagnostics: {:?}",
            r.diagnostics
        );
        let codes = error_codes(&r);
        assert!(
            codes.contains(&expected_code.to_string()),
            "expected error code {expected_code}, got {codes:?} for:\n{src}"
        );
    }
}

#[test]
fn rejects_mismatched_scalars_in_complex() {
    let cases = [
        // float32 real, float64 imag
        "fn main() {\n    var r float32 = 1.0\n    var i float64 = 2.0\n    _ = complex(r, i)\n}\n",
        // float64 real, float32 imag
        "fn main() {\n    var r float64 = 1.0\n    var i float32 = 2.0\n    _ = complex(r, i)\n}\n",
        // f32 vs f64 aliases
        "fn main() {\n    var r f32 = 1.0\n    var i f64 = 2.0\n    _ = complex(r, i)\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected reject for:\n{src}\nDiagnostics: {:?}",
            r.diagnostics
        );
        let codes = error_codes(&r);
        assert!(
            codes.contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for mismatched float scalars in complex, got {codes:?} for:\n{src}"
        );
    }
}

#[test]
fn rejects_invalid_operand_types_to_real_and_imag() {
    let cases = [
        // float64 passed to real
        "fn main() {\n    var x float64 = 1.0\n    _ = real(x)\n}\n",
        // float32 passed to imag
        "fn main() {\n    var x float32 = 1.0\n    _ = imag(x)\n}\n",
        // int32 passed to real
        "fn main() {\n    var x int32 = 1\n    _ = real(x)\n}\n",
        // int passed to imag
        "fn main() {\n    var x int = 1\n    _ = imag(x)\n}\n",
        // string passed to real
        "fn main() {\n    var s string = \"x\"\n    _ = real(s)\n}\n",
        // bool passed to imag
        "fn main() {\n    var b bool = true\n    _ = imag(b)\n}\n",
        // slice passed to real
        "fn main() {\n    var sl []int\n    _ = real(sl)\n}\n",
        // pointer passed to imag
        "fn main() {\n    var p *int\n    _ = imag(p)\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected reject for:\n{src}\nDiagnostics: {:?}",
            r.diagnostics
        );
        let codes = error_codes(&r);
        assert!(
            codes.contains(&"HYB-E2005".to_string()),
            "expected HYB-E2005 for non-complex operand to real/imag, got {codes:?} for:\n{src}"
        );
    }
}

#[test]
fn rejects_mismatched_assigned_and_return_part_types() {
    let cases = [
        // real(complex64) is float32, cannot assign to float64
        "fn main() {\n    var c complex64\n    var r float64 = real(c)\n    _ = r\n}\n",
        // imag(complex128) is float64, cannot assign to float32
        "fn main() {\n    var c complex128\n    var i float32 = imag(c)\n    _ = i\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected reject for:\n{src}\nDiagnostics: {:?}",
            r.diagnostics
        );
        let codes = error_codes(&r);
        assert!(
            codes.contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for scalar width mismatch, got {codes:?} for:\n{src}"
        );
    }
}

#[test]
fn accepts_valid_complex_real_imag_forms() {
    let src = r#"
fn main() {
    // untyped numeric literals
    _ = complex(1.0, 2.0)
    _ = complex(1, 2)
    _ = real(1.0)
    _ = imag(2.0)
    _ = real(complex(1.0, 2.0))

    // matching float32
    var r32 float32 = 1.0
    var i32 float32 = 2.0
    c1 := complex(r32, i32)
    var re32 float32 = real(c1)
    var im32 float32 = imag(c1)
    _ = re32
    _ = im32

    // matching float64
    var r64 float64 = 1.0
    var i64 float64 = 2.0
    c2 := complex(r64, i64)
    var re64 float64 = real(c2)
    var im64 float64 = imag(c2)
    _ = re64
    _ = im64

    // declared complex variables
    var c3 complex64
    var re3 float32 = real(c3)
    var im3 float32 = imag(c3)
    _ = re3
    _ = im3

    var c4 complex128
    var re4 float64 = real(c4)
    var im4 float64 = imag(c4)
    _ = re4
    _ = im4

    // constant literal initialization of complex variables
    var c5 complex64 = complex(1.0, 2.0)
    var c6 complex128 = complex(1.0, 2.0)
    _ = c5
    _ = c6
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected valid forms to accept, got diagnostics: {:?}",
        r.diagnostics
    );
    assert!(
        error_codes(&r).is_empty(),
        "expected 0 errors, got {:?}",
        r.diagnostics
    );
}
