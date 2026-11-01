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
