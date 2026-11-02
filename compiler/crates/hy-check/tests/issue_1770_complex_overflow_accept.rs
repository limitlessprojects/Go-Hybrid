//! Issue #1770: complex conversions follow the float magnitude rules.
//! `complex64(1e40)` overflows float32 and must be HYB-E6006; in-range
//! numeric and rune constants, including `complex128(1e40)`, stay accepted.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects_with_e6006(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6006"),
        "expected HYB-E6006 for {:?}, got {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts_without_e6006(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E6006"),
        "must NOT fire HYB-E6006 for {:?}, got {:?}",
        src,
        cs
    );
}

#[test]
fn complex64_overflow_constant_rejects_issue_1770() {
    rejects_with_e6006("fn main() { x := complex64(1e40) _ = x }");
}

#[test]
fn complex_targets_accept_in_range_constants_issue_1770() {
    accepts_without_e6006("fn main() { x := complex64(1) _ = x }");
    accepts_without_e6006("fn main() { x := complex128(2.5) _ = x }");
    accepts_without_e6006("fn main() { x := complex128(1e40) _ = x }");
    accepts_without_e6006("fn main() { x := complex64('A') _ = x }");
}
