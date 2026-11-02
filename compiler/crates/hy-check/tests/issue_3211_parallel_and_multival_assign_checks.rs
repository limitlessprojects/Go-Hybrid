//! issue_3211_parallel_and_multival_assign_checks.rs — regression tests for Issue #3212:
//! scalar width and fixed-size array length validation in parallel assignment
//! and multi-value call assignment.

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
fn test_issue_3211_assignment_mismatches_rejected() {
    let cases = [
        // 1. parallel assignment with mismatched array variable
        (
            "package main\nfn f() {\n    var a [2]int\n    var b [2]int\n    var u [3]int\n    var v [3]int\n    a, b = u, v\n    _ = a\n    _ = b\n}\nfn main() {}\n",
            "parallel assignment mismatched array variable",
        ),
        // 2. parallel assignment with mismatched array literal
        (
            "package main\nfn f() {\n    var a [2]int\n    var b [2]int\n    a, b = [3]int{1, 2, 3}, [2]int{1, 2}\n    _ = a\n    _ = b\n}\nfn main() {}\n",
            "parallel assignment mismatched array literal",
        ),
        // 3. multi-value call assignment with mismatched scalar width
        (
            "package main\nfn ret_pair() (int64, int32) {\n    var a int64 = 1\n    var b int32 = 2\n    return a, b\n}\nfn f() {\n    var x int32\n    var y int32\n    x, y = ret_pair()\n    _ = x\n    _ = y\n}\nfn main() {}\n",
            "multi-value assignment mismatched scalar width",
        ),
        // 4. multi-value call assignment with mismatched array length
        (
            "package main\nfn ret_pair() (int32, [3]int) {\n    var a int32 = 1\n    var b [3]int\n    return a, b\n}\nfn f() {\n    var x int32\n    var y [2]int\n    x, y = ret_pair()\n    _ = x\n    _ = y\n}\nfn main() {}\n",
            "multi-value assignment mismatched array length",
        ),
    ];

    for (src, desc) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected reject for {desc}, got accept"
        );
        let codes = error_codes(&r);
        assert!(
            codes.contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for {desc}, got: {:?}",
            codes
        );
    }
}

#[test]
fn test_issue_3211_valid_assignments_accepted() {
    let cases = [
        // 1. matching parallel array assignment
        "package main\nfn f() {\n    var a [2]int\n    var b [2]int\n    var u [2]int\n    var v [2]int\n    a, b = u, v\n    _ = a\n    _ = b\n}\nfn main() {}\n",
        // 2. matching multi-value call assignment
        "package main\nfn ret_pair() (int32, [2]int) {\n    var a int32 = 1\n    var b [2]int\n    return a, b\n}\nfn f() {\n    var x int32\n    var y [2]int\n    x, y = ret_pair()\n    _ = x\n    _ = y\n}\nfn main() {}\n",
    ];

    for (i, src) in cases.iter().enumerate() {
        let r = check(src);
        let codes = error_codes(&r);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "case #{i} should accept cleanly, but got errors: {:?}",
            codes
        );
    }
}
