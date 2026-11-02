//! issue_3222_slice_array_lit_checks.rs — regression tests for Issue #3222:
//! fixed-size array element dimensions in slice composite literals and array composite literals.

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
fn test_issue_3222_slice_and_array_lit_rejections() {
    let cases = [
        // 1. slice literal with array variable length mismatch
        (
            "package main\nfn main() {\n    arr3 := [3]int{1, 2, 3}\n    s := [][2]int{arr3}\n    _ = s\n}\n",
            "slice literal with array variable length mismatch",
        ),
        // 2. slice literal with array literal length mismatch
        (
            "package main\nfn main() {\n    s := [][2]int{[3]int{1, 2, 3}}\n    _ = s\n}\n",
            "slice literal with array literal length mismatch",
        ),
        // 3. array literal with array variable length mismatch
        (
            "package main\nfn main() {\n    arr3 := [3]int{1, 2, 3}\n    a := [1][2]int{arr3}\n    _ = a\n}\n",
            "array literal with array variable length mismatch",
        ),
        // 4. array literal with array literal length mismatch
        (
            "package main\nfn main() {\n    a := [1][2]int{[3]int{1, 2, 3}}\n    _ = a\n}\n",
            "array literal with array literal length mismatch",
        ),
        // 5. smaller array into larger array element
        (
            "package main\nfn main() {\n    arr2 := [2]int{1, 2}\n    s := [][3]int{arr2}\n    _ = s\n}\n",
            "smaller array into larger array element in slice literal",
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
fn test_issue_3222_slice_and_array_lit_acceptances() {
    let cases = [
        // 1. matching array variable in slice literal
        (
            "package main\nfn main() {\n    arr2 := [2]int{1, 2}\n    s := [][2]int{arr2}\n    _ = s\n}\n",
            "matching array variable in slice literal",
        ),
        // 2. matching array literal in slice literal
        (
            "package main\nfn main() {\n    s := [][2]int{[2]int{1, 2}, [2]int{3, 4}}\n    _ = s\n}\n",
            "matching array literal in slice literal",
        ),
        // 3. matching array variable and literal in multi-dimensional array literal
        (
            "package main\nfn main() {\n    arr2 := [2]int{1, 2}\n    a := [2][2]int{arr2, [2]int{3, 4}}\n    _ = a\n}\n",
            "matching array variable and literal in array literal",
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
