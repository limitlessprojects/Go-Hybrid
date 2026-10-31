//! `set[T]` / `option[T]` member lookup used to fail open: any method name was
//! accepted by `hyb check`, then `go build` refused `s.bogus undefined`.
//!
//! The prelude generics resolved to `OpKind::Unknown`, so the member-call pass
//! (`HYB-E1002`) skipped them (it only fires on fully-resolved receivers) and
//! `has_method` had no entry for the prelude. They are now resolved named types
//! with a small method table.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let file = parse_file("test.hy", src).expect("parse should succeed");
    check_file(&file)
}

fn rejects_with(src: &str, code: &str) {
    let res = check(src);
    assert_eq!(
        res.verdict,
        Verdict::Reject,
        "expected reject for {code}, got {:?}",
        res.diagnostics
    );
    assert!(
        res.diagnostics.iter().any(|d| d.code == code),
        "expected {code}, got {:?}",
        res.diagnostics
    );
}

#[test]
fn unknown_method_on_set_is_rejected() {
    rejects_with(
        "\
fn main() {
    s := set[int]{}
    _ = s.bogus()
}
",
        "HYB-E1002",
    );
}

#[test]
fn unknown_method_on_option_is_rejected() {
    rejects_with(
        "\
fn main() {
    o := some(5)
    _ = o.bogus()
}
",
        "HYB-E1002",
    );
    rejects_with(
        "\
fn main() {
    var o option[int]
    _ = o.bogus()
}
",
        "HYB-E1002",
    );
}

#[test]
fn prelude_methods_still_resolve() {
    assert_eq!(
        check(
            "\
fn main() {
    s := set[int]{}
    s.insert(1)
    _ = s.contains(1)
    _ = s.len()
    o := some(5)
    _ = o.unwrap()
    _ = o.is_some()
    _ = o.is_none()
    _ = o.or(9)
}
"
        )
        .verdict,
        Verdict::Accept
    );
}

#[test]
fn wrong_arity_on_prelude_method_is_rejected() {
    rejects_with(
        "\
fn main() {
    s := set[int]{}
    s.insert(1, 2)
}
",
        "HYB-E1002",
    );
}

#[test]
fn user_fn_named_set_with_two_args_is_not_a_conversion() {
    // A user `fn set` wins over the prelude type, so a two-argument call is a
    // plain call, not an over-arity conversion to `set`.
    assert_eq!(
        check(
            "\
fn set(dst []map[string]int, v map[string]int) {
    dst[0] = v
}

fn main() {
    dst := make([]map[string]int, 1)
    set(dst, map[string]int{\"k\": 0})
}
"
        )
        .verdict,
        Verdict::Accept
    );
}
