//! issue_1825_append_spread_validation.rs — regression tests for Issue #1825:
//! variadic `append(a, b...)` operands must be checked (HYB-E1002, Row 13 BUILDABLE).
//! Mirrors the issue's reproduction with local `var` bindings and a string literal.

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

fn program(stmt: &str) -> String {
    format!(
        "package main\n\nfn main() {{\n    var nums []int = []int{{1, 2}}\n    var strs []string = []string{{\"a\", \"b\"}}\n    var f []float64 = []float64{{1.0, 2.0}}\n    var x int = 42\n    _ = strs\n    _ = f\n    _ = x\n    {}\n}}\n",
        stmt
    )
}

#[test]
fn test_issue_1825_repro_forms_rejected() {
    let cases = [
        ("[]int <- []string", "_ = append(nums, strs...)"),
        ("[]int <- []float64", "_ = append(nums, f...)"),
        ("[]int <- string literal", "_ = append(nums, \"hello\"...)"),
        ("[]int <- int", "_ = append(nums, x...)"),
    ];

    for (desc, stmt) in cases {
        let r = check(&program(stmt));
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected rejection for {}, got: {:?}",
            desc,
            r.diagnostics
        );
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for {}, got: {:?}",
            desc,
            r.diagnostics
        );
    }
}

#[test]
fn test_issue_1825_controls_accepted() {
    let r = check(&program("_ = append(nums, []int{3}...)"));
    assert_eq!(r.verdict, Verdict::Accept, "got: {:?}", r.diagnostics);

    let src = "package main\n\nfn main() {\n    var b []byte = []byte{1}\n    _ = append(b, \"hi\"...)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "got: {:?}", r.diagnostics);
}
