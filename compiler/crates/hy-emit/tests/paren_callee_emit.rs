//! paren_callee_emit.rs — regression tests for #1991: a parenthesized
//! selector callee must lower exactly like its unparenthesized spelling.
//!
//! `hy-check` peels parentheses before classifying the callee, so every
//! `hy-emit` gate that matches `Expr::Field` directly was bypassed by
//! `(pkg.fn)(...)`, `(recv.method)(...)`, `(xs.pop)()` and `(T.assoc)(...)`.

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("test source must parse");
    emit_go(&ast).expect("test source must emit cleanly")
}

#[test]
fn parenthesized_package_call_lowers_and_imports() {
    let go = emit("fn main() {\n    (fmt.println)(\"hi\")\n}\n");
    assert!(
        go.contains("\"fmt\""),
        "expected the fmt import to be registered for a parenthesized package callee:\n{go}"
    );
    assert!(
        go.contains("fmt.Println("),
        "expected `(fmt.println)(x)` to normalize to `fmt.Println(x)`:\n{go}"
    );
    assert!(
        !go.contains("(fmt.println)"),
        "parenthesized callee leaked into the Go output:\n{go}"
    );
}

#[test]
fn parenthesized_foreign_method_normalizes() {
    let go = emit(
        "struct pool {\n    mu sync.mutex\n}\n\nfn main() {\n    p := pool{}\n    (p.mu.lock)()\n    (p.mu.unlock)()\n}\n",
    );
    assert!(
        go.contains("p.mu.Lock()"),
        "expected `(p.mu.lock)()` to normalize to `p.mu.Lock()`:\n{go}"
    );
    assert!(
        go.contains("p.mu.Unlock()"),
        "expected `(p.mu.unlock)()` to normalize to `p.mu.Unlock()`:\n{go}"
    );
}

#[test]
fn parenthesized_slice_pop_uses_prelude() {
    let go = emit("fn main() {\n    xs := []int{1, 2}\n    v := (xs.pop)()\n    _ = v\n}\n");
    assert!(
        go.contains("hy_pop(&xs)"),
        "expected `(xs.pop)()` to lower to `hy_pop(&xs)`:\n{go}"
    );
    assert!(
        !go.contains("xs.pop()"),
        "raw `.pop()` leaked into the Go output:\n{go}"
    );
}

#[test]
fn parenthesized_associated_fn_is_mangled() {
    let go = emit(
        "struct box[t] {\n    v t\n}\n\nfn box.new[t](v t) box[t] {\n    return box[t]{v: v}\n}\n\nfn main() {\n    b := (box.new)(3)\n    _ = b\n}\n",
    );
    assert!(
        go.contains("box_new(3)"),
        "expected `(box.new)(3)` to mangle to `box_new(3)`:\n{go}"
    );
    assert!(
        !go.contains("(box.new)"),
        "parenthesized assoc callee leaked into the Go output:\n{go}"
    );
}
