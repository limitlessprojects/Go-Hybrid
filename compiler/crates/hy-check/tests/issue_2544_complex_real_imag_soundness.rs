//! Regression tests for #2544:
//! verification of numeric/complex operand kinds (HYB-E2005) and type matching (HYB-E1002)
//! for `complex`, `real`, and `imag`, properly preserving untyped numeric constant compatibility.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> (Verdict, Vec<hy_diag::Diagnostic>) {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    (res.verdict, res.diagnostics)
}

#[test]
fn complex_real_imag_non_numeric_operands_rejected_with_e2005() {
    let cases = [
        // 1. string operands in complex
        r#"
fn main() {
    _ = complex("a", "b")
}
"#,
        // 2. bool operands in complex
        r#"
fn main() {
    _ = complex(true, false)
}
"#,
        // 3. slice operands in complex
        r#"
fn main() {
    s1 := []int{1}
    s2 := []int{2}
    _ = complex(s1, s2)
}
"#,
        // 4. non-numeric string operand in real
        r#"
fn main() {
    _ = real("hello")
}
"#,
        // 5. non-numeric bool operand in imag
        r#"
fn main() {
    _ = imag(true)
}
"#,
        // 6. non-numeric slice operand in real
        r#"
fn main() {
    _ = real([]int{1})
}
"#,
        // 7. non-numeric map operand in imag
        r#"
fn main() {
    _ = imag(map[string]int{})
}
"#,
    ];

    for (idx, src) in cases.iter().enumerate() {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Reject,
            "non-numeric case #{idx} should reject, got Accept:\n{src}"
        );
        assert!(
            diags.iter().any(|d| d.code == "HYB-E2005"),
            "non-numeric case #{idx} should emit HYB-E2005: {diags:?}\n{src}"
        );
    }
}

#[test]
fn complex_mismatched_operand_types_rejected_with_e1002() {
    let cases = [
        // 1. float and non-constant int variable in complex
        r#"
fn main() {
    var i int = 2
    _ = complex(1.5, i)
}
"#,
        // 2. non-constant int variable and float in complex
        r#"
fn main() {
    var i int = 2
    _ = complex(i, 1.5)
}
"#,
        // 3. float and string in complex
        r#"
fn main() {
    _ = complex(1.0, "abc")
}
"#,
    ];

    for (idx, src) in cases.iter().enumerate() {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Reject,
            "mismatched case #{idx} should reject, got Accept:\n{src}"
        );
        let has_expected = diags.iter().any(|d| d.code == "HYB-E1002" || d.code == "HYB-E2005");
        assert!(
            has_expected,
            "mismatched case #{idx} should emit HYB-E1002/E2005: {diags:?}\n{src}"
        );
    }
}

#[test]
fn complex_valid_numeric_operands_accepted() {
    let cases = [
        // 1. float literals in complex
        r#"
fn main() {
    _ = complex(1.5, 2.5)
}
"#,
        // 2. integer literals in complex
        r#"
fn main() {
    _ = complex(1, 2)
}
"#,
        // 3. float literal and untyped int literal in complex (Go-valid!)
        r#"
fn main() {
    _ = complex(1.5, 2)
}
"#,
        // 4. untyped int literal and float literal in complex (Go-valid!)
        r#"
fn main() {
    _ = complex(2, 1.5)
}
"#,
        // 5. untyped int const and float literal in complex
        r#"
const k = 4
fn main() {
    _ = complex(1.5, k)
}
"#,
        // 6. numeric argument to real
        r#"
fn main() {
    _ = real(1.5)
}
"#,
        // 7. numeric argument to imag
        r#"
fn main() {
    _ = imag(2)
}
"#,
        // 8. nested complex call result to real
        r#"
fn main() {
    _ = real(complex(1.0, 2.0))
}
"#,
        // 9. nested complex call result to imag
        r#"
fn main() {
    _ = imag(complex(1.0, 2.0))
}
"#,
    ];

    for (idx, src) in cases.iter().enumerate() {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Accept,
            "valid case #{idx} should accept, got Reject with diags: {diags:?}\n{src}"
        );
    }
}
