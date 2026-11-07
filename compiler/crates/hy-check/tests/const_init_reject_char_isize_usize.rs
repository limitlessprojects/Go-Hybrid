//! Regression tests for rejecting non-Go primitive names (`char`, `isize`, `usize`)
//! in `is_conversion_type_name` (`HYB-E6003` / `HYB-E1002`, Row 13 BUILDABLE).

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn test_const_char_isize_usize_conversions_reject() {
    for src in [
        "const c = char(65)\nfn main() { _ = c }\n",
        "const u = usize(1024)\nfn main() { _ = u }\n",
        "const i = isize(-1)\nfn main() { _ = i }\n",
    ] {
        let r = check(src);
        let codes = error_codes(&r);
        assert!(
            !codes.is_empty(),
            "must reject non-Go conversion in const init, got 0 errors for:\n{}",
            src
        );
        assert_eq!(r.verdict, Verdict::Reject);
    }
}

#[test]
fn test_slice_lit_char_elem_rejects() {
    let src = "fn main() {\n    s := []char{65}\n    _ = s\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_valid_go_const_conversions_accept() {
    let src = r#"
const r = rune(65)
const b = byte(65)
const n = uint(1024)
const m = int(-1)
fn main() {
    _ = r
    _ = b
    _ = n
    _ = m
}
"#;
    let r = check(src);
    assert!(error_codes(&r).is_empty(), "got: {:?}", r.diagnostics);
    assert_eq!(r.verdict, Verdict::Accept);
}