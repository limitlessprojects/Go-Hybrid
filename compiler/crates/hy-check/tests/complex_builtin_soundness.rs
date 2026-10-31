//! complex_builtin_soundness.rs — regression tests for Issue #2544:
//! verification of numeric/complex operand kinds (HYB-E2005) and type matching (HYB-E1002)
//! for `complex`, `real`, and `imag`.

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
fn complex_real_imag_non_numeric_operands_rejected_with_e2005() {
    // 1. string operands in complex
    let src_str = "fn main() {\n    _ = complex(\"a\", \"b\")\n}\n";
    let r_str = check(src_str);
    assert_eq!(r_str.verdict, Verdict::Reject);
    assert!(error_codes(&r_str).contains(&"HYB-E2005".to_string()));

    // 2. bool operands in complex
    let src_bool = "fn main() {\n    _ = complex(true, false)\n}\n";
    let r_bool = check(src_bool);
    assert_eq!(r_bool.verdict, Verdict::Reject);
    assert!(error_codes(&r_bool).contains(&"HYB-E2005".to_string()));

    // 3. slice operands in complex
    let src_slice = "fn main() {\n    s1 := []int{1}\n    s2 := []int{2}\n    _ = complex(s1, s2)\n}\n";
    let r_slice = check(src_slice);
    assert_eq!(r_slice.verdict, Verdict::Reject);
    assert!(error_codes(&r_slice).contains(&"HYB-E2005".to_string()));

    // 4. non-numeric operand in real
    let src_real = "fn main() {\n    _ = real(\"hello\")\n}\n";
    let r_real = check(src_real);
    assert_eq!(r_real.verdict, Verdict::Reject);
    assert!(error_codes(&r_real).contains(&"HYB-E2005".to_string()));

    // 5. non-numeric operand in imag
    let src_imag = "fn main() {\n    _ = imag(true)\n}\n";
    let r_imag = check(src_imag);
    assert_eq!(r_imag.verdict, Verdict::Reject);
    assert!(error_codes(&r_imag).contains(&"HYB-E2005".to_string()));
}

#[test]
fn complex_valid_numeric_operands_accepted() {
    // 1. float constants in complex
    let src_flts = "fn main() {\n    _ = complex(1.5, 2.5)\n}\n";
    let r_flts = check(src_flts);
    assert_eq!(r_flts.verdict, Verdict::Accept);

    // 2. integer constants in complex
    let src_ints = "fn main() {\n    _ = complex(1, 2)\n}\n";
    let r_ints = check(src_ints);
    assert_eq!(r_ints.verdict, Verdict::Accept);

    // 3. numeric argument to real
    let src_real = "fn main() {\n    _ = real(1.5)\n}\n";
    let r_real = check(src_real);
    assert_eq!(r_real.verdict, Verdict::Accept);

    // 4. numeric argument to imag
    let src_imag = "fn main() {\n    _ = imag(2)\n}\n";
    let r_imag = check(src_imag);
    assert_eq!(r_imag.verdict, Verdict::Accept);
}
