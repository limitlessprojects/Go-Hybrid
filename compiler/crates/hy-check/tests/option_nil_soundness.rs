//! Regression tests for option nil soundness (Row 13 BUILDABLE):
//! Reject untyped nil equality comparisons, match patterns, and call arguments
//! on non-nilable `option[T]` types.
//!
//! Under Go specification (§Comparison operators, §Assignability) and Row 13 of GUARANTEES.md:
//! - In Go, `option[T]` lowers to the generic prelude struct `hyOption[T]`.
//! - Struct values are non-nilable and cannot be compared to untyped `nil`.
//! - `hy-check` must reject `opt == nil`, `nil == opt`, and `match opt { nil => ... }`
//!   with HYB-E1002, preventing invalid Go code from escaping check time.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("option_nil_test.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn count_e1002(r: &hy_check::CheckResult) -> usize {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error && d.code == "HYB-E1002")
        .count()
}

#[test]
fn option_equality_with_nil_is_rejected() {
    let cases = [
        "fn main() {\n    o := some(1)\n    if o == nil {\n        _ = 1\n    }\n}\n",
        "fn main() {\n    o := some(1)\n    if nil == o {\n        _ = 1\n    }\n}\n",
        "fn main() {\n    o := some(\"a\")\n    if o != nil {\n        _ = 1\n    }\n}\n",
        "fn main() {\n    o := some(\"a\")\n    if nil != o {\n        _ = 1\n    }\n}\n",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected reject for nil comparison: {src}"
        );
        assert!(
            count_e1002(&r) >= 1,
            "expected HYB-E1002 for nil comparison: {src}, got: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn option_match_nil_pattern_is_rejected() {
    let src = "fn main() {\n    o := some(42)\n    match o {\n        nil => {}\n        _ => {}\n    }\n}\n";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected reject for match nil on option"
    );
    assert!(
        count_e1002(&r) >= 1,
        "expected HYB-E1002 for match nil: {:?}",
        r.diagnostics
    );
}

#[test]
fn option_valid_comparisons_accept() {
    let src = "fn main() {\n    o := some(42)\n    if o == some(42) {\n        _ = 1\n    }\n    if o != none {\n        _ = 2\n    }\n}\n";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept for valid option comparisons: {:?}",
        r.diagnostics
    );
}
