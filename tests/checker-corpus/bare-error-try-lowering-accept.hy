// ============================================================================
// bare-error-try-lowering-accept — Checker corpus (issue #726).
//
// PURPOSE: `?` on a call that returns BARE `error` — the simplest legal E3
// use — must be accepted AND lower to buildable Go.
//
// RULE CITATION: SPEC-LANGUAGE E1 (line 972): "Functions signal failure by
// returning `error` as the final result: `(string, error)`, `(total int, err
// error)`, or bare `error`." E3/E4: `?` applies to an error-returning call
// inside a function whose results end in `error`, or in `main`.
//
// ---- Why this is an ACCEPT ----
// The program is valid and prints `ok`. The bug was in LOWERING: the `?` hoist
// emitted `hyT0, hyErr1 := step()` against a one-value call
// (`assignment mismatch: 2 variables but step returns 1 value`), plus a bare
// `hyT0` statement Go also refuses.
//
// Two causes, both in how the hoist learns a call's arity: `fn_shapes` stored
// `results.len()`, which INCLUDES the trailing `error`, so `fn step() error`
// read as `(1, true)` instead of `(0, true)`; and a method on a USER struct was
// in no table at all. The descriptor tables spell the count the other way
// (`serde_json.value.set` is `(0, true)` — it returns only an error), so only
// user-declared functions and methods hit this.
//
// ---- Why an ACCEPT fixture and not a REJECT one ----
// The checker was RIGHT to accept. `hyb check` reported 0 diagnostics both
// before and after the fix, so this fixture pins the verdict and the emit-level
// tests pin the lowering — `hy-emit/tests/bare_error_try_lowering.rs`. A reject
// fixture here would have been the wrong shape and would have passed for the
// wrong reason.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: ACCEPT
//
// ---- Paired fixture ----
// The accept side of the ordinary two-result form is already covered by
// `control-*` fixtures and by `two_result_call_under_try_keeps_its_value_temp`
// in the emit tests, which pins that the fix did NOT collapse the value temp
// for a call that genuinely has one.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn step() error {
    return nil
}

struct store {
    n int
}

fn (s *store) save() error {
    return nil
}

fn run(s *store) error {
    step()?
    s.save()?
    return nil
}

fn main() {
    var s store
    run(&s)?
    fmt.println("ok")
}
