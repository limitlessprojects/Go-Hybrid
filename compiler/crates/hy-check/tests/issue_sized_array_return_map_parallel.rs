//! Tests for sized-array length mismatches in map index stores, return statements,
//! and parallel assignments (Row 13 BUILDABLE).
//!
//! Under GUARANTEES.md Row 13 (BUILDABLE), any program accepted by `hyb check`
//! must compile under `go build`.
//! Sized arrays with differing lengths (`[2]int` vs `[3]int`) are distinct types in Go
//! and cannot be assigned or returned without explicit conversion:
//! - m["k"] = b (map[string][2]int = [3]int) -> cannot use b (variable of type [3]int) as [2]int value in assignment
//! - return b ([2]int <- [3]int) -> cannot use b (variable of type [3]int) as [2]int value in return statement
//! - a, _ = b, 1 ([2]int <- [3]int) -> cannot use b (variable of type [3]int) as [2]int value in multiple assignment

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "test.hy";
    let ast = parse_file(path, src).unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn errors(r: &hy_check::CheckResult) -> Vec<(String, String)> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| (d.code.clone(), d.message.clone()))
        .collect()
}

fn assert_rejected(src: &str, expected_code: &str, what: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected Reject for {what}, got Accept (0 diagnostics)"
    );
    let errs = errors(&r);
    assert!(
        errs.iter().any(|(code, _)| code == expected_code),
        "expected {expected_code} for {what}, got {errs:?}"
    );
}

fn assert_accepted(src: &str, what: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected Accept for {what}, got Reject with {:#?}",
        r.diagnostics
    );
}

#[test]
fn test_sized_array_length_mismatches_rejected() {
    // 1. Map index assignment with different array length
    assert_rejected(
        "fn f(m map[string][2]int, b [3]int) {\n    m[\"k\"] = b\n}\n",
        "HYB-E1002",
        "map index store with array length mismatch",
    );

    // 2. Map index assignment with inferred array local
    assert_rejected(
        "fn f(m map[string][2]int) {\n    b := [3]int{1, 2, 3}\n    m[\"k\"] = b\n}\n",
        "HYB-E1002",
        "map index store with inferred array length mismatch",
    );

    // 3. Single return statement with different array length
    assert_rejected(
        "fn f(b [3]int) [2]int {\n    return b\n}\n",
        "HYB-E1002",
        "single return with array length mismatch",
    );

    // 4. Multi-value return statement with different array length
    assert_rejected(
        "fn f(b [3]int) (int, [2]int) {\n    return 1, b\n}\n",
        "HYB-E1002",
        "multi-value return with array length mismatch",
    );

    // 5. Return statement with inferred array local
    assert_rejected(
        "fn f() [2]int {\n    b := [3]int{1, 2, 3}\n    return b\n}\n",
        "HYB-E1002",
        "return with inferred array length mismatch",
    );

    // 6. Return statement with array literal
    assert_rejected(
        "fn f() [2]int {\n    return [3]int{1, 2, 3}\n}\n",
        "HYB-E1002",
        "return with array literal length mismatch",
    );

    // 7. Parallel assignment with different array length
    assert_rejected(
        "fn f(a [2]int, b [3]int) {\n    a, _ = b, 1\n}\n",
        "HYB-E1002",
        "parallel assignment with array length mismatch",
    );
}

#[test]
fn test_sized_array_controls_accepted() {
    // Map store with matching array length
    assert_accepted(
        "fn f(m map[string][2]int, a [2]int) {\n    m[\"k\"] = a\n}\n",
        "map store matching [2]int",
    );

    // Return with matching array length
    assert_accepted(
        "fn f(a [2]int) [2]int {\n    return a\n}\n",
        "return matching [2]int",
    );

    // Multi-return with matching array length
    assert_accepted(
        "fn f(a [2]int) (int, [2]int) {\n    return 1, a\n}\n",
        "multi-return matching [2]int",
    );

    // Parallel assignment with matching array length
    assert_accepted(
        "fn f(a [2]int, c [2]int) {\n    a, _ = c, 1\n}\n",
        "parallel assignment matching [2]int",
    );
}
