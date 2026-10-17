//! Regression tests for issue #155 in `hy-emit`:
//!   1. Error-returning closures inside `main()` or another function scope
//!      `in_main_closure` and `cur_fn_error_returns` to the closure's own
//!      declared results (`return <zeros>, hyErrN`), restoring the outer
//!      function's state afterwards.
//!   2. Non-error closures inside `main()` do not inherit `in_main_closure`.
//!   3. Non-error user calls (`(n, false)` in `fn_shapes`) are rejected in
//!      `hoist_one_try` and `emit_try_define`.

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> Result<String, String> {
    let ast = parse_file("test.hy", src).expect("must parse");
    emit_go(&ast).map_err(|e| e.message)
}

#[test]
fn error_returning_closure_in_main_emits_typed_error_return_issue_155() {
    let src = "\
fn load() (int, error) {
    return 7, nil
}

fn main() {
    cb := fn() (int, error) {
        n := load()?
        return n, nil
    }
    x := cb()?
    fmt.println(x)
}
";
    let go = emit(src).expect("must emit");
    assert!(
        go.contains("return 0, hyErr0"),
        "closure returning (int, error) must propagate with `return 0, hyErr0`, got:\n{}",
        go
    );
    assert!(
        go.contains("hyErr = hyErr1"),
        "outer main() must still restore in_main_closure and propagate via `hyErr = hyErr1`, got:\n{}",
        go
    );
}

#[test]
fn non_error_closure_in_main_does_not_inherit_main_error_scope_issue_155() {
    let src = "\
fn fail() error {
    return nil
}

fn main() {
    cb := fn() {
        fail()?
    }
    cb()
}
";
    let err = emit(src).expect_err("non-error closure in main must refuse `?` emission");
    assert!(
        err.contains("`?` outside main"),
        "unexpected error: {}",
        err
    );
}

#[test]
fn non_error_call_try_is_refused_in_emitter_issue_155() {
    let src_define = "\
fn five() int {
    return 5
}

fn main() {
    x := five()?
    fmt.println(x)
}
";
    let err1 = emit(src_define).expect_err("five()? in define must be refused by emitter");
    assert!(
        err1.contains("does not return error"),
        "unexpected error: {}",
        err1
    );

    let src_hoist = "\
fn five() int {
    return 5
}

fn main() {
    fmt.println(five()?)
}
";
    let err2 = emit(src_hoist).expect_err("five()? in hoisted expr must be refused by emitter");
    assert!(
        err2.contains("does not return error"),
        "unexpected error: {}",
        err2
    );
}

#[test]
fn defer_argument_try_hoists_before_defer_issue_155() {
    let src = "\
fn load() (int, error) {
    return 1, nil
}

fn close_it(n int) error {
    _ = n
    return nil
}

fn run() error {
    defer close_it(load()?)
    return nil
}
";
    let go = emit(src).expect("argument-position `?` in defer must hoist cleanly");
    let hoist_pos = go.find("hyT0, hyErr1 := load()").expect("hoisted load()");
    let defer_pos = go.find("defer close_it(hyT0)").expect("defer close_it(hyT0)");
    assert!(
        hoist_pos < defer_pos,
        "hoisted argument `?` must precede `defer close_it(hyT0)`:\n{}",
        go
    );
}

