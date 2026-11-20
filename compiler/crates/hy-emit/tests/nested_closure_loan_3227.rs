//! Regression pin for #3227: a slot loan whose writer is a closure that
//! arrives through a second-order value.
//!
//! The loan `p` is written by a function literal that is never named in the
//! frame that calls it: `mk` builds it and `h` holds it. Without an alias
//! edge from `h` back to `p`, the emitter flushes the copy-back at the end
//! of the statement that binds `h`, and the later call writes a dead copy.

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let ast = parse_file("nested_closure_loan_3227.hy", src).expect("parse");
    emit_go(&ast).expect("emit_go")
}

/// The reported shape: the writer is returned by another closure. The
/// copy-back must not be flushed before the statement that binds `h`.
#[test]
fn nested_closure_returned_loan_is_not_stored_back_early() {
    let go = emit(
        r#"
struct cell { n int }

fn main() {
    xs := []cell{cell{n: 1}}
    p := &xs[0]
    mk := fn() fn() {
        return fn() { p.n = 8 }
    }
    h := mk()
    h()
    fmt.println(xs[0].n)
}
"#,
    );
    // The copy-back must appear after the call, not between the bind and it.
    let bind = go.find("h := mk()").expect("bind emitted");
    let store = go.find("xs[0] = p").expect("copy-back emitted");
    let call = go.find("h()").expect("call emitted");
    assert!(
        call < store,
        "copy-back at {store} must follow the writer call at {call}\n{go}"
    );
    assert!(bind < call, "bind must precede the call\n{go}");
}

/// A statement between the binding and the call: the flush cannot be keyed
/// to "immediately after the decl".
#[test]
fn nested_closure_with_gap_still_keeps_the_write() {
    let go = emit(
        r#"
struct cell { n int }

fn main() {
    xs := []cell{cell{n: 1}}
    p := &xs[0]
    mk := fn() fn() {
        return fn() { p.n = 8 }
    }
    h := mk()
    fmt.println("between")
    h()
    fmt.println(xs[0].n)
}
"#,
    );
    let store = go.find("xs[0] = p").expect("copy-back emitted");
    let call = go.find("h()").expect("call emitted");
    assert!(call < store, "copy-back must follow the writer\n{go}");
}

/// Direct literal capture, the #3194 shape, keeps its edge.
#[test]
fn direct_closure_capture_is_not_stored_back_early() {
    let go = emit(
        r#"
struct cell { n int }

fn main() {
    xs := []cell{cell{n: 1}}
    p := &xs[0]
    f := fn() { p.n = 2 }
    f()
    fmt.println(xs[0].n)
}
"#,
    );
    let store = go.find("xs[0] = p").expect("copy-back emitted");
    let call = go.find("f()").expect("call emitted");
    assert!(call < store, "copy-back must follow the writer\n{go}");
}

/// Two closures over one loan: the flush waits for the last of them.
#[test]
fn two_closures_over_one_loan_both_hold_the_edge() {
    let go = emit(
        r#"
struct cell { n int }

fn main() {
    xs := []cell{cell{n: 0}, cell{n: 0}}
    for i in 0..2 {
        p := &xs[i]
        a := fn() { p.n += 1 }
        b := fn() { p.n += 10 }
        a()
        b()
    }
    fmt.println(xs[0].n)
}
"#,
    );
    let last_call = go.rfind("b()").expect("b call emitted");
    let store = go.rfind("xs[i] = p").expect("copy-back emitted");
    assert!(
        last_call < store,
        "copy-back must follow the last writer\n{go}"
    );
}

/// A closure handed to another function, which calls it.
#[test]
fn closure_passed_to_a_fn_is_not_stored_back_early() {
    let go = emit(
        r#"
struct cell { n int }

fn apply(f fn()) {
    f()
}

fn main() {
    xs := []cell{cell{n: 1}}
    p := &xs[0]
    g := fn() { p.n = 5 }
    apply(g)
    fmt.println(xs[0].n)
}
"#,
    );
    let store = go.find("xs[0] = p").expect("copy-back emitted");
    let call = go.find("apply(g)").expect("call emitted");
    assert!(call < store, "copy-back must follow the writer\n{go}");
}
