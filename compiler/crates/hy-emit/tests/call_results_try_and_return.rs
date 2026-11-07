//! Regression tests for `stmt.rs` (`call_results`, `Stmt::Expr`, and `emit_return`)
//! covering:
//! 1. `s.step()?` on user-defined struct methods, `S.step()?` on associated functions,
//!    `step[int]()?` on generic functions, `(fn() error { return nil })()?` on inline
//!    closures, and `os.remove("x")?` on 1-result `error` stdlib calls in expression
//!    / statement position.
//! 2. Statement-position `val_step()?` (1-value + `error` call) emitting `_ = hyT0`
//!    instead of a bare unused expression `hyT0`.
//! 3. `return inner()` in an error-returning function `fn outer() (int, error)` not
//!    appending a spurious `, nil` when `inner()` already returns `(int, error)`.

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let file = parse_file("test.hy", src).expect("parses");
    emit_go(&file).expect("emits")
}

#[test]
fn zero_value_error_calls_in_try_emit_single_error_assignment() {
    let go = emit(
        r#"import "fmt"
import "os"

struct worker {}

fn (w worker) ping() error {
    return nil
}

fn worker.static_ping() error {
    return nil
}

fn gen_ping[t any]() error {
    return nil
}

fn run(w worker) error {
    w.ping()?
    worker.static_ping()?
    gen_ping[int]()?
    (fn() error { return nil })()?
    os.remove("tmp.txt")?
    return nil
}

fn main() {
    fmt.println(1)
}
"#,
    );

    assert!(
        !go.contains("hyT"),
        "0-value error calls with `?` must not emit a 2-var `hyT, hyErr := ...` assignment:\n{go}"
    );
    assert!(
        go.contains(":= w.ping()")
            && go.contains(":= worker_static_ping()")
            && go.contains(":= gen_ping[int]()")
            && go.contains(":= os.Remove(\"tmp.txt\")"),
        "expected single-var error assignments for method, assoc fn, generic fn, and os.remove:\n{go}"
    );
}

#[test]
fn statement_position_value_try_discards_temp_and_forwarded_error_return_omits_extra_nil() {
    let go = emit(
        r#"import "fmt"

fn step() (int, error) {
    return 42
}

fn outer() (int, error) {
    step()?
    return step()
}

fn main() {
    fmt.println(1)
}
"#,
    );

    assert!(
        go.contains("_ = hyT0"),
        "statement-position `step()?` must discard `hyT0` with `_ = hyT0`:\n{go}"
    );
    assert!(
        go.contains("return step()\n") && !go.contains("return step(), nil"),
        "`return step()` in `(int, error)` fn must not append `, nil`:\n{go}"
    );
}
