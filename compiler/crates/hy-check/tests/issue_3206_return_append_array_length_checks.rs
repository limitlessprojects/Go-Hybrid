//! issue_3206_return_append_array_length_checks.rs — regression tests for Issue #3206:
//! fixed-size array length validation in return statements, built-in append(),
//! and slice/array composite literals.

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
fn test_issue_3206_array_length_mismatch_rejected() {
    let cases = [
        // 1. return statement mismatch with variable
        (
            "package main\nfn f() [2]int {\n    var a [3]int\n    return a\n}\nfn main() {}\n",
            "return mismatched array variable",
        ),
        // 2. return statement mismatch with array literal
        (
            "package main\nfn f() [2]int {\n    return [3]int{1, 2, 3}\n}\nfn main() {}\n",
            "return mismatched array literal",
        ),
        // 3. append mismatch with variable
        (
            "package main\nfn f() {\n    var s [][2]int\n    var a [3]int\n    s = append(s, a)\n    _ = s\n}\nfn main() {}\n",
            "append mismatched array variable",
        ),
        // 4. append mismatch with array literal
        (
            "package main\nfn f() {\n    var s [][2]int\n    s = append(s, [3]int{1, 2, 3})\n    _ = s\n}\nfn main() {}\n",
            "append mismatched array literal",
        ),
        // 5. append spread mismatch
        (
            "package main\nfn f() {\n    var s [][2]int\n    var extra [][3]int\n    s = append(s, extra...)\n    _ = s\n}\nfn main() {}\n",
            "append spread mismatched slice of array",
        ),
        // 6. slice literal mismatch with variable
        (
            "package main\nfn f() {\n    var a [3]int\n    var s = [][2]int{a}\n    _ = s\n}\nfn main() {}\n",
            "slice literal item mismatched variable",
        ),
        // 7. slice literal mismatch with literal
        (
            "package main\nfn f() {\n    var s = [][2]int{[3]int{1, 2, 3}}\n    _ = s\n}\nfn main() {}\n",
            "slice literal item mismatched literal",
        ),
        // 8. array literal mismatch with variable
        (
            "package main\nfn f() {\n    var a [3]int\n    var arr = [1][2]int{a}\n    _ = arr\n}\nfn main() {}\n",
            "array literal item mismatched variable",
        ),
        // 9. array literal mismatch with literal
        (
            "package main\nfn f() {\n    var arr = [1][2]int{[3]int{1, 2, 3}}\n    _ = arr\n}\nfn main() {}\n",
            "array literal item mismatched literal",
        ),
        // 10. closure return mismatch
        (
            "package main\nfn f() {\n    _ = fn() [2]int {\n        var a [3]int\n        return a\n    }\n}\nfn main() {}\n",
            "closure return mismatched array variable",
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
fn test_issue_3206_matching_array_accepted() {
    let cases = [
        // 1. matching return
        "package main\nfn f() [2]int {\n    var a [2]int\n    return a\n}\nfn main() {}\n",
        // 2. matching return literal
        "package main\nfn f() [2]int {\n    return [2]int{1, 2}\n}\nfn main() {}\n",
        // 3. matching append
        "package main\nfn f() {\n    var s [][2]int\n    var a [2]int\n    s = append(s, a)\n    _ = s\n}\nfn main() {}\n",
        // 4. matching append literal
        "package main\nfn f() {\n    var s [][2]int\n    s = append(s, [2]int{1, 2})\n    _ = s\n}\nfn main() {}\n",
        // 5. matching slice literal
        "package main\nfn f() {\n    var a [2]int\n    var s = [][2]int{a, [2]int{1, 2}}\n    _ = s\n}\nfn main() {}\n",
        // 6. matching array literal
        "package main\nfn f() {\n    var a [2]int\n    var arr = [2][2]int{a, [2]int{1, 2}}\n    _ = arr\n}\nfn main() {}\n",
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
