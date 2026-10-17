//! #455 — a store performed inside a declared helper must materialize the
//! caller-side may_share edge, so a later `move` freezes the alias the helper
//! created.
//!
//! `fn set(dst []map[string]int, v map[string]int) { dst[0] = v }` is analysed
//! in the callee's own `FnState`: the caller's graph never learns that `dst[0]`
//! aliases `v`. Moving `v` into a spawned task then leaves `dst` writable
//! (`fatal error: concurrent map writes`). The fix records the callee's
//! argument-store effect at the call site.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn first_error_rule(r: &hy_check::CheckResult) -> Option<String> {
    r.diagnostics
        .iter()
        .find(|d| d.severity == Severity::Error)
        .and_then(|d| d.tier0_rule.clone())
}

fn assert_reject_e3003(src: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(first_error_rule(&r).as_deref(), Some("A1b"));
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003, got {:?}",
        r.diagnostics
    );
}

#[test]
fn callee_store_alias_freezes_source_on_move_e3003_455() {
    assert_reject_e3003(
        r#"
        fn set(dst []map[string]int, v map[string]int) {
            dst[0] = v
        }
        fn worker(m map[string]int) {
            for i in 1..=200000 { m["task"]++ }
        }
        fn main() {
            shared := map[string]int{}
            dst := []map[string]int{map[string]int{}}
            set(dst, shared)
            task worker(move shared)
            for i in 1..=200000 { dst[0]["main"]++ }
        }
        "#,
    );
}

#[test]
fn callee_store_through_local_alias_rejects() {
    assert_reject_e3003(
        r#"
        fn set(dst []map[string]int, v map[string]int) {
            tmp := dst
            tmp[0] = v
        }
        fn worker(m map[string]int) {
            for i in 1..=200000 { m["task"]++ }
        }
        fn main() {
            shared := map[string]int{}
            dst := []map[string]int{map[string]int{}}
            set(dst, shared)
            task worker(move shared)
            for i in 1..=200000 { dst[0]["main"]++ }
        }
        "#,
    );
}

#[test]
fn callee_store_in_nested_block_rejects() {
    assert_reject_e3003(
        r#"
        fn set(dst []map[string]int, v map[string]int) {
            if len(dst) > 0 {
                dst[0] = v
            }
        }
        fn worker(m map[string]int) {
            for i in 1..=200000 { m["task"]++ }
        }
        fn main() {
            shared := map[string]int{}
            dst := []map[string]int{map[string]int{}}
            set(dst, shared)
            task worker(move shared)
            for i in 1..=200000 { dst[0]["main"]++ }
        }
        "#,
    );
}

#[test]
fn callee_store_two_level_call_rejects() {
    assert_reject_e3003(
        r#"
        fn inner(dst []map[string]int, v map[string]int) {
            dst[0] = v
        }
        fn outer(dst []map[string]int, v map[string]int) {
            inner(dst, v)
        }
        fn worker(m map[string]int) {
            for i in 1..=200000 { m["task"]++ }
        }
        fn main() {
            shared := map[string]int{}
            dst := []map[string]int{map[string]int{}}
            outer(dst, shared)
            task worker(move shared)
            for i in 1..=200000 { dst[0]["main"]++ }
        }
        "#,
    );
}

#[test]
fn callee_store_closure_body_rejects() {
    assert_reject_e3003(
        r#"
        fn set(dst []map[string]int, v map[string]int) {
            f := fn() {
                dst[0] = v
            }
            f()
        }
        fn worker(m map[string]int) {
            for i in 1..=200000 { m["task"]++ }
        }
        fn main() {
            shared := map[string]int{}
            dst := []map[string]int{map[string]int{}}
            set(dst, shared)
            task worker(move shared)
            for i in 1..=200000 { dst[0]["main"]++ }
        }
        "#,
    );
}

#[test]
fn callee_without_store_accepts() {
    // Over-rejection guard: a declared call that only reads both maps must not
    // create a may-share edge between them.
    let r = check(
        r#"
        fn pick(a map[string]int, b map[string]int) int {
            if len(a) > len(b) { return len(a) }
            return len(b)
        }
        fn worker(m map[string]int) {
            for i in 1..=200000 { m["task"]++ }
        }
        fn main() {
            a := map[string]int{}
            b := map[string]int{}
            _ = pick(a, b)
            task worker(move a)
            for i in 1..=200000 { b["main"]++ }
        }
        "#,
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn direct_store_control_still_rejects() {
    assert_reject_e3003(
        r#"
        fn worker(m map[string]int) {
            for i in 1..=200000 { m["task"]++ }
        }
        fn main() {
            shared := map[string]int{}
            dst := []map[string]int{map[string]int{}}
            dst[0] = shared
            task worker(move shared)
            for i in 1..=200000 { dst[0]["main"]++ }
        }
        "#,
    );
}
