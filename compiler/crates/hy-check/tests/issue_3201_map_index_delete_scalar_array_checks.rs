//! Regression tests for issue #3201:
//! Map indexing, built-in `delete(m, k)`, and map composite literals
//! must enforce fixed-width scalar widths and sized array lengths under HYB-E1002 / HYB-E2005.

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
fn issue_3201_rejects_map_mismatches() {
    let cases = [
        // 1. Map index with int32 key and int64 index
        (
            "fn main() {\n    var m map[int32]string\n    var k int64 = 10\n    _ = m[k]\n}\n",
            "map index with int64 variable on map[int32]string",
            "HYB-E1002",
        ),
        // 2. Map index with float32 key and float64 index
        (
            "fn main() {\n    var m map[float32]string\n    var k float64 = 3.14\n    _ = m[k]\n}\n",
            "map index with float64 variable on map[float32]string",
            "HYB-E1002",
        ),
        // 3. Map index with signed int key and unsigned uint index
        (
            "fn main() {\n    var m map[int]string\n    var k uint = 10\n    _ = m[k]\n}\n",
            "map index with uint variable on map[int]string",
            "HYB-E1002",
        ),
        // 4. Map index with sized array length mismatch ([2]int vs [3]int)
        (
            "fn main() {\n    var m map[[2]int]string\n    var k [3]int\n    _ = m[k]\n}\n",
            "map index with [3]int variable on map[[2]int]string",
            "HYB-E1002",
        ),
        // 5. Map index assignment with int32 key and int64 index
        (
            "fn main() {\n    var m map[int32]string\n    var k int64 = 10\n    m[k] = \"hello\"\n}\n",
            "map index assignment with int64 variable on map[int32]string",
            "HYB-E1002",
        ),
        // 6. Map index assignment with sized array length mismatch
        (
            "fn main() {\n    var m map[[2]int]string\n    var k [3]int\n    m[k] = \"hello\"\n}\n",
            "map index assignment with [3]int variable on map[[2]int]string",
            "HYB-E1002",
        ),
        // 7. delete(m, k) with int32 key and int64 argument
        (
            "fn main() {\n    var m map[int32]string\n    var k int64 = 10\n    delete(m, k)\n}\n",
            "delete with int64 argument on map[int32]string",
            "HYB-E2005",
        ),
        // 8. delete(m, k) with float32 key and float64 argument
        (
            "fn main() {\n    var m map[float32]string\n    var k float64 = 3.14\n    delete(m, k)\n}\n",
            "delete with float64 argument on map[float32]string",
            "HYB-E2005",
        ),
        // 9. delete(m, k) with sized array length mismatch
        (
            "fn main() {\n    var m map[[2]int]string\n    var k [3]int\n    delete(m, k)\n}\n",
            "delete with [3]int argument on map[[2]int]string",
            "HYB-E2005",
        ),
        // 10. Map literal with sized array key variable mismatch
        (
            "fn main() {\n    var k [3]int\n    _ = map[[2]int]string{ k: \"hello\" }\n}\n",
            "map literal key with [3]int variable for [2]int key type",
            "HYB-E1002",
        ),
        // 11. Map literal with sized array key literal mismatch
        (
            "fn main() {\n    _ = map[[2]int]string{ [3]int{1, 2, 3}: \"hello\" }\n}\n",
            "map literal key with [3]int literal for [2]int key type",
            "HYB-E1002",
        ),
        // 12. Map literal with sized array value variable mismatch
        (
            "fn main() {\n    var v [3]int\n    _ = map[string][2]int{ \"k\": v }\n}\n",
            "map literal value with [3]int variable for [2]int value type",
            "HYB-E1002",
        ),
        // 13. Map literal with sized array value literal mismatch
        (
            "fn main() {\n    _ = map[string][2]int{ \"k\": [3]int{1, 2, 3} }\n}\n",
            "map literal value with [3]int literal for [2]int value type",
            "HYB-E1002",
        ),
    ];

    for (src, desc, expected_code) in cases {
        let parsed = parse_file("test.hy", src).expect("parse must succeed");
        let result = check_file(&parsed);
        assert_eq!(
            result.verdict,
            Verdict::Reject,
            "Expected Reject for {desc}, but got Accept.\nSource:\n{src}"
        );
        let codes = error_codes(&result);
        assert!(
            codes.contains(&expected_code.to_string()),
            "Expected {expected_code} error for {desc}, got: {codes:?}\nSource:\n{src}"
        );
    }
}

#[test]
fn issue_3201_accepts_valid_map_cases() {
    let cases = [
        // 1. Map index with matching int32 key
        (
            "fn main() {\n    var m map[int32]string\n    var k int32 = 10\n    _ = m[k]\n}\n",
            "map index with matching int32 key",
        ),
        // 2. Map index with untyped constant
        (
            "fn main() {\n    var m map[int32]string\n    _ = m[10]\n}\n",
            "map index with untyped int constant",
        ),
        // 3. Map index with matching sized array key
        (
            "fn main() {\n    var m map[[2]int]string\n    var k [2]int\n    _ = m[k]\n}\n",
            "map index with matching [2]int key",
        ),
        // 4. delete(m, k) with matching int32 key
        (
            "fn main() {\n    var m map[int32]string\n    var k int32 = 10\n    delete(m, k)\n}\n",
            "delete with matching int32 key",
        ),
        // 5. delete(m, k) with matching sized array key
        (
            "fn main() {\n    var m map[[2]int]string\n    var k [2]int\n    delete(m, k)\n}\n",
            "delete with matching [2]int key",
        ),
        // 6. Map literal with matching sized array keys and values
        (
            "fn main() {\n    var k [2]int\n    var v [2]int\n    _ = map[[2]int][2]int{ k: v }\n}\n",
            "map literal with matching [2]int key and value",
        ),
        // 7. Map literal with matching sized array literals
        (
            "fn main() {\n    _ = map[[2]int][2]int{ [2]int{1, 2}: [2]int{3, 4} }\n}\n",
            "map literal with matching [2]int literals",
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
