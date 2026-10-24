//! defer_nested_scope_and_rebind.rs — Regression tests for Issue #1393:
//! 1. Deferred calls inside nested blocks (`if`/`for`/`match`/`select`) run at
//!    function exit and must be checked against frozen/consumed bindings.
//! 2. `defer fn() { ... }()` closure captures must be scanned by
//!    `collect_deferred_uses_in_stmts` so post-spawn deferred reads/writes
//!    trigger HYB-E3003.
//! 3. Rebinding an identifier (`a = fresh`) after `b := a` must not clear
//!    existing `may_share` peer edges between `a` and `b`.

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

fn assert_e3003(src: &str, what: &str) {
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3003"),
        "{}: expected HYB-E3003, got {:?}",
        what,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "{}", what);
}

#[test]
fn defer_in_nested_if_block_touching_moved_map_is_e3003() {
    assert_e3003(
        "fn Touch(m map[string]int) { m["a"] = 1 }
         fn Worker(own m map[string]int) { m["b"] = 2 }
         fn main() {
             let mut m = map[string]int{"init": 0}
             if true {
                 defer Touch(m)
             }
             t := task Worker(move m)
             await t
         }
",
        "defer inside nested if block runs at fn exit after move m",
    );
}

#[test]
fn defer_closure_capturing_moved_map_is_e3003() {
    assert_e3003(
        "fn Worker(own m map[string]int) { m["b"] = 2 }
         fn main() {
             let mut m = map[string]int{"init": 0}
             defer fn() {
                 m["race"] = 1
             }()
             t := task Worker(move m)
             await t
         }
",
        "defer closure literal capturing m runs at fn exit after move m",
    );
}

#[test]
fn rebind_after_shallow_copy_preserves_may_share_peer_on_move() {
    assert_e3003(
        "struct Holder { s []int }
         fn Worker(own h Holder) { h.s[0] = 99 }
         fn main() {
             let mut a = Holder{s: []int{1, 2, 3}}
             let mut b = a
             a = Holder{s: []int{4, 5, 6}}
             t := task Worker(move b)
             println(a.s[0])
             await t
         }
",
        "rebind of a after b := a preserves may_share edge when moving b",
    );
}
