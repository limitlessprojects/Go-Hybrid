//! call_share.rs — regression tests for issue #73: a define whose value is
//! a CALL only recorded sharing when the result shape was a reference
//! alias (#38's `record_alias` Call arm), so a shallow struct copy through a
//! call and a generic identity (result shape Unknown through the type
//! parameter) created an untracked peer: the later `move` froze only the
//! original binding, and the write through the peer passed with 0
//! diagnostics while the emitted Go raced (`fatal error: concurrent map
//! writes`). `record_may_share` now has a Call arm (same
//! no-interprocedural-summary argument corpus 26 pinned for the alias
//! path); the generic form is pinned at the corpus level by
//! 28-generic-call-return-alias-freeze.hy.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

const STRUCT_COPY_THROUGH_CALL: &str = "\
struct holder {
    m map[string]int
}

fn alias_holder(h holder) holder {
    return h
}

fn worker(h holder, done chan<- bool) {
    for i in 0..10000 {
        h.m[\"k\"] = i
    }
    done <- true
}

fn main() {
    h1 := holder{m: map[string]int{\"k\": 0}}
    h2 := alias_holder(h1)
    done := make(chan bool, 1)
    task worker(move h1, done)
    for i in 0..10000 {
        h2.m[\"k\"] = i
    }
    <-done
}
";

#[test]
fn struct_copy_through_call_is_frozen_e3003() {
    // `h2 := alias_holder(h1)` is a shallow struct copy: the result shape
    // is Struct — not a binding-level alias — so only the may-share graph
    // can track it. Before the Call arm it was untracked: 0 diagnostics
    // while `go run` reported `fatal error: concurrent map writes`.
    let r = check(STRUCT_COPY_THROUGH_CALL);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 for the write through the call-returned peer, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const GENERIC_IDENTITY_THROUGH_CALL: &str = "\
fn id[t any](x t) t {
    return x
}

fn worker(m map[string]int, done chan<- bool) {
    for i in 0..10000 {
        m[\"k\"] = i
    }
    done <- true
}

fn main() {
    m := map[string]int{\"k\": 0}
    m2 := id(m)
    done := make(chan bool, 1)
    task worker(move m, done)
    for i in 0..10000 {
        m2[\"k\"] = i
    }
    <-done
}
";

#[test]
fn generic_identity_through_call_is_frozen_e3003() {
    // `id[m]`'s result type is the type PARAMETER `t` → Unknown, which is
    // not a reference alias, so #38's recorder skipped it too. Corpus 28
    // pins the send-form; this unit test pins the spawn-form twin.
    let r = check(GENERIC_IDENTITY_THROUGH_CALL);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 for the generic-identity peer, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn scalar_result_through_call_stays_copy_out() {
    // Precision control: a concrete scalar result is a COPY out of the
    // callee (O4/A4) — no sharing claim, no edge, so moving the argument
    // must not freeze the result binding.
    let r = check(
        "\
fn wrap(m map[string]int) int {
    return len(m)
}

fn worker(m map[string]int, done chan<- bool) {
    m[\"k\"] = 1
    done <- true
}

fn main() {
    m := map[string]int{\"k\": 0}
    x := wrap(m)
    done := make(chan bool, 1)
    task worker(move m, done)
    y := x + 1
    _ = y
    <-done
}
",
    );
    assert!(
        codes(&r).is_empty(),
        "a scalar copy-out must stay untracked, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn lexical_struct_copy_is_still_frozen_e3003() {
    // The #27/#35 direct-copy form must keep working after the Call arm
    // lands (twin of STRUCT_COPY_THROUGH_CALL without the call).
    let r = check(
        "\
struct holder {
    m map[string]int
}

fn worker(h holder, done chan<- bool) {
    h.m[\"k\"] = 1
    done <- true
}

fn main() {
    h1 := holder{m: map[string]int{\"k\": 0}}
    h2 := h1
    done := make(chan bool, 1)
    task worker(move h1, done)
    h2.m[\"k\"] = 2
    <-done
}
",
    );
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 for the direct-copy peer (regression), got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}
