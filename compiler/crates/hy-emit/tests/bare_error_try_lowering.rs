//! #726 — `f()?` on a user function or method that returns bare `error`.
//!
//! SPEC E1: "Functions signal failure by returning `error` as the final
//! result: `(string, error)`, `(total int, err error)`, **or bare `error`**."
//! SPEC E3/E4: `?` applies to an error-returning call inside a function whose
//! results end in `error`, or in `main`. So `step()?` on a `fn step() error`
//! is the simplest legal use — the Hybrid spelling of
//! `if err := step(); err != nil { return err }`.
//!
//! `hyb check` accepted it, and the emitted Go did not compile:
//!
//! ```go
//! func run() error {
//!     hyT0, hyErr1 := step()   // step returns ONE value
//!     if hyErr1 != nil { return hyErr1 }
//!     hyT0                     // and: evaluated but not used
//!     return nil
//! }
//! ```
//!
//! Two causes, both in how the `?` hoist learns a call's arity:
//!
//!   1. `fn_shapes` stored `results.len()`, which INCLUDES the trailing
//!      `error`, so `fn step() error` recorded `(1, true)` and never matched
//!      the dedicated zero-value path (`(0, true)`). The descriptor tables
//!      spell the count the other way — `serde_json.value.set` is
//!      `(0, true)`, it returns only an error — so only user-declared
//!      functions hit this.
//!   2. A method on a USER struct was not in the tables at all, so
//!      `s.save()?` found no entry and fell through to the two-name form.
//!
//! These tests drive the emitter end to end (build the file, inspect the
//! emitted Go), because the bug is a lowering bug: the checker was right to
//! accept, and asserting on diagnostics alone would have passed before the
//! fix.

use hy_emit::emit_go;
use hy_syntax::parse_file;

/// Emit a program and return the generated Go.
fn emit(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("parses");
    emit_go(&ast).expect("emits")
}

fn assert_emits_ok(src: &str) -> String {
    let go = emit(src);
    assert!(!go.is_empty(), "expected emitted Go for:\n{src}");
    go
}

// ---------------------------------------------------------------------------
// The reported shapes: a bare-`error` callee under `?` must lower to a
// single-name assignment.
// ---------------------------------------------------------------------------

#[test]
fn bare_error_fn_under_try_emits_one_name() {
    let go = assert_emits_ok(
        "fn step() error {\n    return nil\n}\n\nfn run() error {\n    step()?\n    return nil\n}\n\nfn main() {\n    run()?\n    fmt.println(\"ok\")\n}\n",
    );
    // The broken form was `hyT0, hyErr1 := step()`. The value temp must not
    // exist for a callee that returns only an error.
    assert!(
        !go.contains("hyT0, hyErr1 := step()"),
        "two-name assignment against a one-value call:\n{go}"
    );
    assert!(
        go.contains("hyErr") && go.contains(":= step()"),
        "expected a single-name error temp for `step()`:\n{go}"
    );
}

#[test]
fn bare_error_fn_with_args_under_try_emits_one_name() {
    let go = assert_emits_ok(
        "fn validate(n int) error {\n    return nil\n}\n\nfn run(n int) error {\n    validate(n)?\n    return nil\n}\n\nfn main() {\n    run(1)?\n    fmt.println(\"ok\")\n}\n",
    );
    // Exactly the broken shape: `hyT0, hyErr1 := validate(n)`.
    assert!(
        !go.contains("hyT0, hyErr1 := validate(n)"),
        "two-name assignment against `validate(n)`:\n{go}"
    );
    assert!(
        go.contains(":= validate(n)"),
        "expected a single-name error temp for `validate(n)`:\n{go}"
    );
}

#[test]
fn bare_error_method_under_try_emits_one_name() {
    // A method on a USER struct is not in the descriptor tables, so it needs
    // its own registry entry (#726 part 2).
    let go = assert_emits_ok(
        "struct store {\n    n int\n}\n\nfn (s *store) save() error {\n    return nil\n}\n\nfn run(s *store) error {\n    s.save()?\n    return nil\n}\n\nfn main() {\n    var s store\n    run(&s)?\n    fmt.println(\"ok\")\n}\n",
    );
    assert!(
        !go.contains("hyT0, hyErr"),
        "two-name assignment against a one-value method call:\n{go}"
    );
}

#[test]
fn bare_error_call_in_main_under_try_emits_one_name() {
    let go = assert_emits_ok(
        "fn step() error {\n    return nil\n}\n\nfn main() {\n    step()?\n    fmt.println(\"ok\")\n}\n",
    );
    assert!(
        !go.contains("hyT0, hyErr"),
        "two-name assignment in main's E6 wrapper:\n{go}"
    );
}

// ---------------------------------------------------------------------------
// The accept half: the ordinary two-result forms must keep their value temp.
// ---------------------------------------------------------------------------

#[test]
fn two_result_call_under_try_keeps_its_value_temp() {
    let go = assert_emits_ok(
        "fn parse(s string) (int, error) {\n    return 0, nil\n}\n\nfn main() {\n    v := parse(\"1\")?\n    fmt.println(\"{v}\")\n}\n",
    );
    // This one genuinely HAS a value result, so the two-name form is correct
    // and must not be collapsed by the fix.
    assert!(
        go.contains("hyErr"),
        "expected an error temp for a two-result call:\n{go}"
    );
}

#[test]
fn named_result_call_under_try_still_emits() {
    let go = assert_emits_ok(
        "fn load() (n int, err error) {\n    return 1, nil\n}\n\nfn main() {\n    v := load()?\n    fmt.println(\"{v}\")\n}\n",
    );
    assert!(!go.is_empty());
}

#[test]
fn plain_call_without_try_is_unaffected() {
    let go = assert_emits_ok(
        "fn step() error {\n    return nil\n}\n\nfn main() {\n    err := step()\n    if err != nil {\n        fmt.println(\"bad\")\n    }\n}\n",
    );
    assert!(
        go.contains(":= step()"),
        "a plain (non-`?`) call must still bind its error:\n{go}"
    );
}
