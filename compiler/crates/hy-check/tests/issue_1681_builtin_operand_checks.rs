//! Regression tests for issue #1681: `cap` on map literals, `len`/`cap` on
//! untyped `nil`, and `delete` on non-map literal first arguments (HYB-E2005).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let file = parse_file("issue_1681.hy", src).expect("source parses");
    check_file(&file)
}

#[test]
fn issue_1681_corpus_reject_and_accept_fixtures() {
    let reject_src = include_str!("../../../../tests/corpus/issue_1681_delete_copy_clear_reject.hy");
    let reject_res = check(reject_src);
    assert_eq!(reject_res.verdict, Verdict::Reject);
    assert!(
        reject_res.diagnostics.iter().all(|d| d.code == "HYB-E2005"),
        "expected only HYB-E2005 diagnostics, got {:?}",
        reject_res.diagnostics
    );
    assert_eq!(reject_res.diagnostics.len(), 7);

    let accept_src = include_str!("../../../../tests/corpus/issue_1681_delete_copy_clear_accept.hy");
    let accept_res = check(accept_src);
    assert_eq!(
        accept_res.verdict,
        Verdict::Accept,
        "expected accept, got {:?}",
        accept_res.diagnostics
    );
}

#[test]
fn issue_1681_parenthesized_and_struct_literal_operands_rejected() {
    for src in [
        r#"fn main() { _ = cap((map[int]int{1: 2})) }"#,
        r#"fn main() { _ = len((nil)) }"#,
        r#"fn main() { _ = cap((nil)) }"#,
        r#"struct s { x int } fn main() { delete(s{x: 1}, "x") }"#,
    ] {
        let res = check(src);
        assert_eq!(res.verdict, Verdict::Reject, "expected reject for `{src}`");
        assert!(
            res.diagnostics.iter().any(|d| d.code == "HYB-E2005"),
            "expected HYB-E2005 for `{src}`, got {:?}",
            res.diagnostics
        );
    }
}
