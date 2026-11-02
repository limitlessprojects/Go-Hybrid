//! issue_3232_map_index_scalar_array_checks.rs — regression tests for Issue #3232:
//! scalar width and fixed-size array length matching in map index expressions (`m[k]`).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("snippet must parse");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.to_string())
        .collect()
}

#[test]
fn test_issue_3232_map_index_rejections() {
    let cases = [
        // 1. map index scalar width mismatch with variable (int64 -> int32)
        (
            "package main\nfn main() {\n    x := int64(1)\n    m := map[int32]string{1: \"a\"}\n    _ = m[x]\n}\n",
            "map index scalar width mismatch int64 to int32",
        ),
        // 2. map index float width mismatch with variable (float64 -> float32)
        (
            "package main\nfn main() {\n    f := float64(1.0)\n    m := map[float32]string{1.0: \"a\"}\n    _ = m[f]\n}\n",
            "map index float width mismatch float64 to float32",
        ),
        // 3. map index array length mismatch with variable ([3]int -> [2]int)
        (
            "package main\nfn main() {\n    arr3 := [3]int{1, 2, 3}\n    m := map[[2]int]string{[2]int{1, 2}: \"a\"}\n    _ = m[arr3]\n}\n",
            "map index array length mismatch with variable",
        ),
        // 4. map index array length mismatch with literal ([3]int -> [2]int)
        (
            "package main\nfn main() {\n    m := map[[2]int]string{[2]int{1, 2}: \"a\"}\n    _ = m[[3]int{1, 2, 3}]\n}\n",
            "map index array length mismatch with literal",
        ),
        // 5. var-declared map index array length mismatch
        (
            "package main\nfn main() {\n    arr3 := [3]int{1, 2, 3}\n    var m map[[2]int]string\n    _ = m[arr3]\n}\n",
            "var-declared map index array length mismatch",
        ),
    ];

    for (src, label) in cases {
        let res = check(src);
        let codes = error_codes(&res);
        assert_eq!(
            res.verdict,
            Verdict::Reject,
            "{label}: expected Verdict::Reject, got {:?}, diagnostics: {:?}",
            res.verdict,
            res.diagnostics
        );
        assert!(
            codes.iter().any(|c| c == "HYB-E1002"),
            "{label}: expected HYB-E1002 in {:?}, diagnostics: {:?}",
            codes,
            res.diagnostics
        );
    }
}

#[test]
fn test_issue_3232_map_index_acceptances() {
    let cases = [
        // 1. matching scalar in map index
        (
            "package main\nfn main() {\n    x := int32(1)\n    m := map[int32]string{1: \"a\"}\n    _ = m[x]\n}\n",
            "matching int32 in map index",
        ),
        // 2. matching array variable in map index
        (
            "package main\nfn main() {\n    arr2 := [2]int{1, 2}\n    m := map[[2]int]string{[2]int{1, 2}: \"a\"}\n    _ = m[arr2]\n}\n",
            "matching array variable in map index",
        ),
        // 3. matching array literal in map index
        (
            "package main\nfn main() {\n    m := map[[2]int]string{[2]int{1, 2}: \"a\"}\n    _ = m[[2]int{1, 2}]\n}\n",
            "matching array literal in map index",
        ),
        // 4. matching array variable on var-declared map
        (
            "package main\nfn main() {\n    arr2 := [2]int{1, 2}\n    var m map[[2]int]string\n    _ = m[arr2]\n}\n",
            "matching array variable on var-declared map",
        ),
    ];

    for (src, label) in cases {
        let res = check(src);
        assert_eq!(
            res.verdict,
            Verdict::Accept,
            "{label}: expected Verdict::Accept, got {:?}, diagnostics: {:?}",
            res.verdict,
            res.diagnostics
        );
        assert!(
            res.diagnostics.is_empty(),
            "{label}: expected 0 diagnostics, got: {:?}",
            res.diagnostics
        );
    }
}
