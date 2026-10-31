//! Issue #1770: `complex64` / `complex128` conversions must reject
//! non-numeric constants (string, bool, nil) with HYB-E6006, the same
//! kind check the numeric conversion targets already apply.

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

#[test]
fn complex_targets_reject_non_numeric_constants_issue_1770() {
    rejects_with_e6006("fn main() { x := complex64(\"invalid\") _ = x }");
    rejects_with_e6006("fn main() { x := complex128(\"invalid\") _ = x }");
    rejects_with_e6006("fn main() { x := complex64(true) _ = x }");
    rejects_with_e6006("fn main() { x := complex128(false) _ = x }");
    rejects_with_e6006("fn main() { x := complex64(nil) _ = x }");
    rejects_with_e6006("fn main() { x := complex128(nil) _ = x }");
}
