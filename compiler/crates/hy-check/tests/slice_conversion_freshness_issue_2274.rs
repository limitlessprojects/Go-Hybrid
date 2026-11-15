//! slice_conversion_freshness_issue_2274.rs — []byte(s), []rune(s), and allocating
//! slice conversions create fresh backing arrays, making them sendable across tasks.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn bytes_from_string_literal_is_sendable() {
    let r = check(
        "fn worker(b []byte) {
             b[0] = 1
         }
         fn main() {
             task worker([]byte(\"abc\"))
         }",
    );
    assert!(
        codes(&r).is_empty(),
        "expected 0 errors for []byte(\"abc\"), got: {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn runes_from_string_literal_is_sendable() {
    let r = check(
        "fn worker(r []rune) {
             r[0] = 'x'
         }
         fn main() {
             task worker([]rune(\"abc\"))
         }",
    );
    assert!(
        codes(&r).is_empty(),
        "expected 0 errors for []rune(\"abc\"), got: {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn bytes_from_string_var_is_sendable() {
    let r = check(
        "fn worker(b []byte) {
             b[0] = 1
         }
         fn main() {
             s := \"hello\"
             task worker([]byte(s))
         }",
    );
    assert!(
        codes(&r).is_empty(),
        "expected 0 errors for []byte(s), got: {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn slice_conversion_ascription_and_move_accepted() {
    let r = check(
        "fn worker(b []byte) {
             b[0] = 1
         }
         fn main() {
             s := \"abc\"
             var x []byte = []byte(s)
             task worker(move x)
         }",
    );
    assert!(
        codes(&r).is_empty(),
        "expected 0 errors for var x []byte = []byte(s); move x, got: {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}
