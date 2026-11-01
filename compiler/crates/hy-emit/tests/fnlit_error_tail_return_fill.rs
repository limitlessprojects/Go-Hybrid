//! fnlit_error_tail_return_fill.rs — a `return` inside a function literal whose
//! last result is `error` must zero-fill to the declared arity, exactly like a
//! top-level error-tail fn (Row 13 BUILDABLE).
//!
//! The literal sets its OWN `cur_fn_error_returns` (expr.rs replaces it for the
//! literal), but `emit_return`'s function-literal branch printed the bare
//! `return` without completing the arity, so `fn() (int, error) { return }`
//! lowered to Go that fails `not enough return values`.

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit_src(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("parses");
    emit_go(&ast).expect("emits")
}

#[test]
fn bare_return_in_error_tail_literal_is_zero_filled() {
    let go = emit_src(
        "fn main() {\n    a, b := (fn() (int, error) {\n        return\n    })()\n    fmt.println(a)\n    if b != nil {\n        fmt.println(\"e\")\n    }\n}\n",
    );
    assert!(
        go.contains("return 0, nil"),
        "the literal's bare return must zero-fill to `return 0, nil`, got:\n{go}"
    );
}

#[test]
fn error_only_literal_bare_return_becomes_nil() {
    let go = emit_src(
        "fn main() {\n    f := fn() error {\n        return\n    }\n    if f() != nil {\n        fmt.println(\"e\")\n    }\n}\n",
    );
    assert!(
        go.contains("return nil"),
        "an `error`-only literal's bare return must become `return nil`, got:\n{go}"
    );
}

#[test]
fn short_return_in_error_tail_literal_is_zero_filled() {
    let go = emit_src(
        "fn main() {\n    a, b := (fn() (int, error) {\n        return 7\n    })()\n    fmt.println(a)\n    if b != nil {\n        fmt.println(\"e\")\n    }\n}\n",
    );
    assert!(go.contains("return 7, nil"), "got:\n{go}");
}

#[test]
fn literal_without_error_tail_keeps_plain_return() {
    // A literal with no error result must not gain a trailing `nil`.
    let go = emit_src("fn main() {\n    f := fn() int {\n        return 3\n    }\n    fmt.println(f())\n}\n");
    assert!(go.contains("return 3"), "got:\n{go}");
    assert!(!go.contains("return 3, nil"), "got:\n{go}");
}
