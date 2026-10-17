//! closure_return_capture.rs — regression tests for the interprocedural tail
//! of the #109 fix: a call whose RESULT is a closure hands back a value that
//! holds live references to the may-share ARGUMENTS it captured in the
//! callee's frame. The #109 fix only inspected `Expr::FnLit` written inline
//! (`f := fn() { ...m... }`); a closure manufactured by a helper
//! (`fn make_fn(m map[string]int) fn() { return fn() { m["k"] = 1 } }`) is a
//! plain `Expr::Call` at the define site — `TypeShape::Closure` is not
//! may-share and a call has no root ident, so `record_may_share`'s carries
//! gate returned early, `f := make_fn(m)` recorded NO edge, and
//! `task worker(move m)` left `f()` unfrozen: the checker accepted the
//! program (0 diagnostics) while the emitted Go died with
//! `fatal error: concurrent map writes`. The fix adds closure-typed call
//! results to the carries gate, so `collect_ref_roots` (which already walks
//! call arguments) links `f` to every may-share argument — the same
//! O5.2/A1b freeze the inline literal gets.

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

// ---------------------------------------------------------------------------
// Rejects — the returned closure is a frozen alias after the move (A1b)
// ---------------------------------------------------------------------------

#[test]
fn returned_closure_called_after_move_spawn_is_e3003() {
    // The spawn form: `task worker(move m)` then `f()`.
    let r = check(
        "fn make_fn(m map[string]int) fn() {\n\
         \x20   return fn() {\n\
         \x20       m[\"k\"] = 1\n\
         \x20   }\n\
         }\n\n\
         fn worker(m map[string]int, done chan<- bool) {\n\
         \x20   m[\"k\"] = 1\n\
         \x20   done <- true\n\
         }\n\n\
         fn main() {\n\
         \x20   m := map[string]int{\"k\": 0}\n\
         \x20   f := make_fn(m)\n\
         \x20   done := make(chan bool, 1)\n\
         \x20   task worker(move m, done)\n\
         \x20   f()\n\
         }\n",
    );
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 for the post-move call of the returned closure, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn returned_closure_called_after_move_send_is_e3003() {
    // The channel form (the #109 shape, laundered through the helper).
    let r = check(
        "fn make_fn(m map[string]int) fn() {\n\
         \x20   return fn() {\n\
         \x20       m[\"k\"] = 1\n\
         \x20   }\n\
         }\n\n\
         fn consumer(ch <-chan map[string]int) {\n\
         \x20   m := <-ch\n\
         \x20   m[\"y\"] = 1\n\
         }\n\n\
         fn main() {\n\
         \x20   a := map[string]int{}\n\
         \x20   f := make_fn(a)\n\
         \x20   ch := make(chan map[string]int, 1)\n\
         \x20   task consumer(ch)\n\
         \x20   ch <- move a\n\
         \x20   f()\n\
         }\n",
    );
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 for the post-move call of the returned closure (channel form), got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn returned_closure_moved_into_task_freezes_argument() {
    // The mirror direction: the closure binding itself is moved across the
    // spawn, so the argument it captured must freeze with it (moving a
    // closure binding does not move or freeze its captured state — the
    // moving side may not keep writing the capture either).
    let r = check(
        "fn make_fn(m map[string]int) fn() {\n\
         \x20   return fn() {\n\
         \x20       m[\"k\"] = 1\n\
         \x20   }\n\
         }\n\n\
         fn runner(f fn(), done chan<- bool) {\n\
         \x20   f()\n\
         \x20   done <- true\n\
         }\n\n\
         fn main() {\n\
         \x20   m := map[string]int{\"k\": 0}\n\
         \x20   f := make_fn(m)\n\
         \x20   done := make(chan bool, 1)\n\
         \x20   task runner(move f, done)\n\
         \x20   m[\"k\"] = 2\n\
         \x20   <-done\n\
         }\n",
    );
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3001" || c == "HYB-E3003"),
        "expected a boundary rejection when the captured map keeps being written after the closure moved, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

// ---------------------------------------------------------------------------
// Accepts — precision: no move, primitive-only arguments, capture-free use
// ---------------------------------------------------------------------------

#[test]
fn returned_closure_without_move_accepts() {
    // No boundary crossing: the closure and the map stay on one task (A5).
    let r = check(
        "fn make_fn(m map[string]int) fn() {\n\
         \x20   return fn() {\n\
         \x20       m[\"k\"] = 1\n\
         \x20   }\n\
         }\n\n\
         fn main() {\n\
         \x20   m := map[string]int{\"k\": 0}\n\
         \x20   f := make_fn(m)\n\
         \x20   f()\n\
         \x20   fmt.println(\"{len(m)}\")\n\
         }\n",
    );
    assert!(
        codes(&r).is_empty(),
        "no move, no boundary — must stay accepted (A5): {:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn returned_closure_primitive_argument_accepts() {
    // A closure over a primitive argument copies the value: no may-share
    // root exists, so the define records no edge and the spawn + call is a
    // legitimate program (the receiver owns its copy).
    let r = check(
        "fn make_adder(base int) fn() {\n\
         \x20   return fn() {\n\
         \x20       base = base + 1\n\
         \x20   }\n\
         }\n\n\
         fn worker(n int, done chan<- bool) {\n\
         \x20   done <- true\n\
         }\n\n\
         fn main() {\n\
         \x20   n := 0\n\
         \x20   f := make_adder(n)\n\
         \x20   done := make(chan bool, 1)\n\
         \x20   task worker(move n, done)\n\
         \x20   f()\n\
         \x20   <-done\n\
         }\n",
    );
    assert!(
        codes(&r).is_empty(),
        "a primitive argument creates no may-share edge: {:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Accept);
}
