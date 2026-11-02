//! Regression tests for issue #3193:
//! Built-in `copy(dst, src)` and composite literals must enforce fixed-width
//! scalar widths and array lengths under HYB-E1002.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn error_codes(res: &hy_check::CheckResult) -> Vec<String> {
    res.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.to_string())
        .collect()
}

#[test]
fn issue_3193_rejects_copy_and_literal_mismatches() {
    let cases = [
        // 1. copy(dst, src) with int32 vs int64 scalar mismatch
        (
            "fn main() {\n    var dst []int32\n    var src []int64\n    copy(dst, src)\n}\n",
            "copy with []int32 destination and []int64 source",
        ),
        // 2. copy(dst, src) with float32 vs float64 scalar mismatch
        (
            "fn main() {\n    var dst []float32\n    var src []float64\n    copy(dst, src)\n}\n",
            "copy with []float32 destination and []float64 source",
        ),
        // 3. copy(dst, src) with signed vs unsigned mismatch
        (
            "fn main() {\n    var dst []int\n    var src []uint\n    copy(dst, src)\n}\n",
            "copy with []int destination and []uint source",
        ),
        // 4. copy(dst, str) where dst is []int instead of []byte
        (
            "fn main() {\n    var dst []int\n    copy(dst, \"hello\")\n}\n",
            "copying string into []int destination",
        ),
        // 5. copy(dst, str) where dst is []int64 instead of []byte
        (
            "fn main() {\n    var dst []int64\n    copy(dst, \"hello\")\n}\n",
            "copying string into []int64 destination",
        ),
        // 6. copy(dst, str) where dst is []rune (int32) instead of []byte
        (
            "fn main() {\n    var dst []rune\n    copy(dst, \"hello\")\n}\n",
            "copying string into []rune destination",
        ),
        // 7. copy(dst, src) with sized array length mismatch ([2]int vs [3]int)
        (
            "fn main() {\n    var dst [][2]int\n    var src [][3]int\n    copy(dst, src)\n}\n",
            "copy with [][2]int destination and [][3]int source",
        ),
        // 8. Slice conversion []int32(xs) where xs is []int64
        (
            "fn main() {\n    var xs []int64\n    ys := []int32(xs)\n}\n",
            "slice conversion from []int64 to []int32",
        ),
        // 9. Slice conversion [][2]int(xs) where xs is [][3]int
        (
            "fn main() {\n    var xs [][3]int\n    ys := [][2]int(xs)\n}\n",
            "slice conversion from [][3]int to [][2]int",
        ),
        // 10. Slice composite literal with mismatched sized array variable
        (
            "fn main() {\n    var a [3]int\n    _ = [][2]int{a}\n}\n",
            "slice literal with element [3]int in [][2]int",
        ),
        // 11. Slice composite literal with mismatched sized array literal
        (
            "fn main() {\n    _ = [][2]int{ [3]int{1, 2, 3} }\n}\n",
            "slice literal with element [3]int literal in [][2]int",
        ),
        // 12. Array composite literal with mismatched sized array variable
        (
            "fn main() {\n    var a [3]int\n    _ = [1][2]int{a}\n}\n",
            "array literal with element [3]int in [1][2]int",
        ),
        // 13. Array composite literal with mismatched sized array literal
        (
            "fn main() {\n    _ = [1][2]int{ [3]int{1, 2, 3} }\n}\n",
            "array literal with element [3]int literal in [1][2]int",
        ),
    ];

    for (src, desc) in cases {
        let parsed = parse_file("test.hy", src).expect("parse must succeed");
        let result = check_file(&parsed);
        assert_eq!(
            result.verdict,
            Verdict::Reject,
            "Expected Reject for {desc}, but got Accept.\nSource:\n{src}"
        );
        let codes = error_codes(&result);
        assert!(
            codes.contains(&"HYB-E1002".to_string()),
            "Expected HYB-E1002 error for {desc}, got: {codes:?}\nSource:\n{src}"
        );
    }
}

#[test]
fn issue_3193_accepts_valid_copy_and_literal_cases() {
    let cases = [
        // 1. copy with matching element types
        (
            "fn main() {\n    var dst []int32\n    var src []int32\n    copy(dst, src)\n}\n",
            "copy with identical []int32 slices",
        ),
        // 2. copy string into []byte
        (
            "fn main() {\n    var dst []byte\n    copy(dst, \"hello\")\n}\n",
            "copying string into []byte slice",
        ),
        // 3. copy string into []uint8
        (
            "fn main() {\n    var dst []uint8\n    copy(dst, \"hello\")\n}\n",
            "copying string into []uint8 slice",
        ),
        // 4. copy with matching sized array slice elements
        (
            "fn main() {\n    var dst [][2]int\n    var src [][2]int\n    copy(dst, src)\n}\n",
            "copy with identical [][2]int slices",
        ),
        // 5. Slice composite literal with matching sized array variable
        (
            "fn main() {\n    var a [2]int\n    _ = [][2]int{a}\n}\n",
            "slice literal with matching [2]int variable",
        ),
        // 6. Array composite literal with matching sized array variable
        (
            "fn main() {\n    var a [2]int\n    _ = [1][2]int{a}\n}\n",
            "array literal with matching [2]int variable",
        ),
        // 7. Array composite literal with matching sized array literal
        (
            "fn main() {\n    _ = [1][2]int{ [2]int{1, 2} }\n}\n",
            "array literal with matching [2]int literal",
        ),
    ];

    for (src, desc) in cases {
        let parsed = parse_file("test.hy", src).expect("parse must succeed");
        let result = check_file(&parsed);
        let codes = error_codes(&result);
        assert_eq!(
            result.verdict,
            Verdict::Accept,
            "Expected Accept for {desc}, but got Reject with errors {codes:?}.\nSource:\n{src}"
        );
    }
}
