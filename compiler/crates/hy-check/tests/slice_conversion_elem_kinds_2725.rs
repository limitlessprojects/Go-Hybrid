//! slice_conversion_elem_kinds_2725.rs — issue #2725 (Row 13 BUILDABLE).
//!
//! A Go slice conversion requires the source and target slice element types to
//! be identical (`[]string([]int)` is `cannot convert ... to type []string`).
//! The named-conversion path (`conversion_kinds_conflict`) already enforced
//! this, but a slice-LITERAL conversion (`[]string(xs)`) reached a different
//! arm that only checked the operand was a slice, so cross-kind conversions
//! (`[]string([]int)`, `[]int([]float64)`, `[]bool([]int)`) passed `hyb check`
//! with 0 diagnostics and failed `go build`.

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
fn slice_conversions_with_mismatched_element_kinds_reject_e1002() {
    for (name, src) in [
        (
            "[]string([]int)",
            "fn main() {\n    s := []int{1, 2}\n    _ = []string(s)\n}\n",
        ),
        (
            "[]int([]float64)",
            "fn main() {\n    f := []float64{1.0}\n    _ = []int(f)\n}\n",
        ),
        (
            "[]bool([]int)",
            "fn main() {\n    s := []int{1}\n    _ = []bool(s)\n}\n",
        ),
        (
            "[]float64([]string)",
            "fn main() {\n    s := []string{\"a\"}\n    _ = []float64(s)\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "{name}");
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "{name}: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn matching_and_unknown_element_kinds_stay_accepted() {
    for (name, src) in [
        (
            "[]int([]int)",
            "fn main() {\n    s := []int{1, 2}\n    _ = []int(s)\n}\n",
        ),
        (
            "[]byte(string)",
            "fn main() {\n    s := \"hi\"\n    _ = []byte(s)\n}\n",
        ),
        (
            "[]rune(string)",
            "fn main() {\n    s := \"hi\"\n    _ = []rune(s)\n}\n",
        ),
        (
            "[]float64([]float64)",
            "fn main() {\n    f := []float64{1.0}\n    _ = []float64(f)\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "{name}: {:?}", r.diagnostics);
        assert!(error_codes(&r).is_empty(), "{name}: {:?}", r.diagnostics);
    }
}

// #1838: OpKind::Int/Float erase widths and signedness, but Go slice
// conversions require identical elements, not merely convertible elements.
#[test]
fn slice_conversions_with_different_numeric_elements_reject_e1002() {
    let mut accepted = Vec::new();
    for (source, target) in [
        ("int", "byte"),
        ("int", "rune"),
        ("int64", "int"),
        ("uint8", "int8"),
        ("float32", "float64"),
        ("[]int64", "[]int"),
        ("[2]int64", "[2]int"),
        ("[010]int", "[10]int"),
    ] {
        let src = format!("fn main() {{\n    var s []{source}\n    _ = []{target}(s)\n}}\n");
        let r = check(&src);
        if r.verdict != Verdict::Reject || !error_codes(&r).contains(&"HYB-E1002".to_string()) {
            accepted.push(format!("[]{target}([]{source}): {:?}", r.diagnostics));
        }
    }
    assert!(accepted.is_empty(), "missing rejection: {accepted:#?}");
}

#[test]
fn identical_numeric_slice_elements_and_aliases_stay_accepted() {
    for (source, target) in [
        ("int", "int"),
        ("uint8", "byte"),
        ("byte", "u8"),
        ("int32", "rune"),
        ("rune", "i32"),
        ("int64", "i64"),
        ("float32", "f32"),
        ("[]int64", "[]i64"),
        ("[2]int64", "[2]i64"),
        ("[01]int", "[1]int"),
        ("[010]int", "[8]int"),
    ] {
        let src = format!("fn main() {{\n    var s []{source}\n    _ = []{target}(s)\n}}\n");
        let r = check(&src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "[]{target}([]{source}): {:?}",
            r.diagnostics
        );
        assert!(error_codes(&r).is_empty(), "{:?}", r.diagnostics);
    }
}
