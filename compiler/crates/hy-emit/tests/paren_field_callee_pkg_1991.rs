//! Regression tests for #1991: parenthesized package, foreign method, unbound-crate, and try-hoist callees.

use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn parenthesized_pkg_and_foreign_method_callees_lower_cleanly() {
    let src = "struct pool {\n\
               \tmu sync.mutex\n\
               }\n\
               fn main() {\n\
               \tp := pool{}\n\
               \t(p.mu.lock)()\n\
               \t(p.mu.unlock)()\n\
               \t(fmt.println)(1)\n\
               }\n";
    let ast = parse_file("test.hy", src).expect("must parse");
    let go = emit_go(&ast).expect("must emit cleanly");
    assert!(
        go.contains("p.mu.Lock()") && go.contains("p.mu.Unlock()"),
        "expected M4 foreign method normalization on parenthesized callee, got:\n{go}"
    );
    assert!(
        go.contains("fmt.Println(1)") && go.contains("\"fmt\""),
        "expected package call normalization and fmt import on (fmt.println)(1), got:\n{go}"
    );
}

#[test]
fn parenthesized_unbound_crate_callee_is_refused() {
    let src = "fn main() {\n\
               \t_ = (flate2.new_writer)(6)?\n\
               }\n";
    let ast = parse_file("test.hy", src).expect("must parse");
    let err = emit_go(&ast).expect_err("unbound crate flate2 must be refused even when parenthesized");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("flate2"),
        "expected unbound crate error mentioning flate2, got: {msg}"
    );
}

#[test]
fn parenthesized_method_callee_hoists_receiver_before_try_arg() {
    let src = "struct sink {}\n\
               fn (s sink) send(x int) {}\n\
               fn get_sink() sink {\n\
               \treturn sink{}\n\
               }\n\
               fn fallible() (int, error) {\n\
               \treturn 7, nil\n\
               }\n\
               fn main() {\n\
               \t(get_sink().send)(fallible()?)\n\
               }\n";
    let ast = parse_file("test.hy", src).expect("must parse");
    let go = emit_go(&ast).expect("must emit cleanly");
    let recv_pos = go.find(":= get_sink()").expect("receiver call must be hoisted to a temp");
    let try_pos = go.find(":= fallible()").expect("try argument must be hoisted");
    assert!(
        recv_pos < try_pos,
        "expected receiver temp before fallible() try hoist, got:\n{go}"
    );
}
