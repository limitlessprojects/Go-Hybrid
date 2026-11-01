//! untyped_nil_index_bounds_2753.rs — issue #2753 (Row 13 BUILDABLE).
//!
//! `hyb check` accepted an untyped `nil` in index and slice-bound positions:
//! `m[nil]` on a non-nilable key map, `xs[nil]`, and `xs[nil:1]`. `nil`
//! resolves to `OpKind::Unknown`, so the integer/index checks failed open and
//! `go build` rejected the emitted Go (`use of untyped nil in index`).

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

#[test]
fn untyped_nil_index_and_bounds_reject_e1002() {
    for (name, src) in [
        (
            "map[int]int nil index",
            "fn main() {\n    m := map[int]int{1: 2}\n    _ = m[nil]\n}\n",
        ),
        (
            "slice nil index",
            "fn main() {\n    s := []int{1, 2}\n    _ = s[nil]\n}\n",
        ),
        (
            "slice nil lo bound",
            "fn main() {\n    s := []int{1, 2}\n    _ = s[nil:1]\n}\n",
        ),
        (
            "slice nil hi bound",
            "fn main() {\n    s := []int{1, 2}\n    _ = s[0:nil]\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "{name}");
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "{name}: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn nilable_map_key_and_ordinary_indices_stay_accepted() {
    for (name, src) in [
        (
            "map[*int] with nil key (nilable)",
            "fn main() {\n    m := map[*int]int{}\n    _ = m[nil]\n}\n",
        ),
        (
            "ordinary slice index",
            "fn main() {\n    s := []int{1, 2}\n    _ = s[1]\n}\n",
        ),
        (
            "ordinary slice bounds",
            "fn main() {\n    s := []int{1, 2}\n    _ = s[0:2]\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "{name}: {:?}", r.diagnostics);
        assert!(error_codes(&r).is_empty(), "{name}: {:?}", r.diagnostics);
    }
}
