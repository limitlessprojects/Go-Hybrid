//! Regression tests for issue #1682: reject non-boolean constant conversions
//! to `bool` and aliased `bool` conversion targets with `HYB-E6006`.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let file = parse_file("issue_1682.hy", src).expect("source parses");
    check_file(&file)
}

#[test]
fn issue_1682_corpus_reject_and_accept_fixtures() {
    let reject_src = include_str!("../../../../tests/corpus/issue_1682_const_bool_conv_reject.hy");
    let reject_res = check(reject_src);
    assert_eq!(reject_res.verdict, Verdict::Reject);
    let e6006_count = reject_res
        .diagnostics
        .iter()
        .filter(|d| d.code == "HYB-E6006")
        .count();
    assert_eq!(
        e6006_count, 8,
        "expected 8 HYB-E6006 diagnostics, got {:?}",
        reject_res.diagnostics
    );

    let accept_src = include_str!("../../../../tests/corpus/issue_1682_const_bool_conv_accept.hy");
    let accept_res = check(accept_src);
    assert_eq!(
        accept_res.verdict,
        Verdict::Accept,
        "expected accept, got {:?}",
        accept_res.diagnostics
    );
}
