//! Tests for #3209: a slot loan captured by a deferred closure must have its
//! store-back deferred (in LIFO order before the user's defer) so that writes
//! inside the deferred closure are not lost to an earlier inline store-back.

use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn test_defer_slot_loan_storeback_slice_issue_3209() {
    let src = r#"
package main
import "fmt"
struct cell { n int }
fn main() {
    xs := []cell{cell{n: 1}}
    run(xs)
    fmt.println(xs[0].n)
}
fn run(xs []cell) {
    p := &xs[0]
    defer fn() { p.n = 7 }()
}
"#;
    let ast = parse_file("d1.hy", src).expect("parses");
    let go = emit_go(&ast).expect("emits");

    // In run, we expect:
    // 1. defer func() { xs[0] = p }()
    // 2. defer func() { p.n = 7 }()
    // with the store-back defer appearing BEFORE the user's defer so that Go's
    // LIFO defer order executes the mutation first, then the store-back.
    let store_defer_pos = go.find("defer func() { xs[0] = p }()").expect("has deferred store-back");
    let user_defer_pos = go.find("p.n = 7").expect("has user deferred closure");
    assert!(
        store_defer_pos < user_defer_pos,
        "store-back defer must precede user defer for LIFO execution order, got:\n{}",
        go
    );
}

#[test]
fn test_defer_slot_loan_direct_write_after_defer() {
    let src = r#"
package main
struct cell { n int }
fn run(xs []cell) {
    p := &xs[0]
    defer fn() { p.n = 7 }()
    p.n = 3
}
"#;
    let ast = parse_file("d2.hy", src).expect("parses");
    let go = emit_go(&ast).expect("emits");
    let store_defer_pos = go.find("defer func() { xs[0] = p }()").expect("has deferred store-back");
    let user_defer_pos = go.find("p.n = 7").expect("has user deferred closure");
    let direct_write_pos = go.find("p.n = 3").expect("has direct write");
    let inline_store_pos = go.rfind("xs[0] = p").expect("has inline store-back");
    assert!(store_defer_pos < user_defer_pos);
    assert!(user_defer_pos < direct_write_pos);
    assert!(direct_write_pos < inline_store_pos);
}

#[test]
fn test_defer_slot_loan_map() {
    let src = r#"
package main
struct cell { n int }
fn run(m map[string]cell) {
    p := &m["k"]
    defer fn() { p.n = 42 }()
}
"#;
    let ast = parse_file("d3.hy", src).expect("parses");
    let go = emit_go(&ast).expect("emits");
    let store_defer_pos = go.find("defer func() { m[\"k\"] = p }()").expect("has deferred map store-back");
    let user_defer_pos = go.find("p.n = 42").expect("has user deferred closure");
    assert!(store_defer_pos < user_defer_pos);
}

#[test]
fn test_defer_named_closure_slot_loan() {
    let src = r#"
package main
struct cell { n int }
fn run(xs []cell) {
    p := &xs[0]
    f := fn() { p.n = 99 }
    defer f()
}
"#;
    let ast = parse_file("d4.hy", src).expect("parses");
    let go = emit_go(&ast).expect("emits");
    let store_defer_pos = go.find("defer func() { xs[0] = p }()").expect("has deferred store-back");
    let defer_f_pos = go.find("defer f()").expect("has defer f()");
    assert!(store_defer_pos < defer_f_pos);
}

#[test]
fn test_defer_multiple_slot_loans() {
    let src = r#"
package main
struct cell { n int }
fn run(xs []cell, ys []cell) {
    p := &xs[0]
    q := &ys[0]
    defer fn() {
        p.n = 10
        q.n = 20
    }()
}
"#;
    let ast = parse_file("d5.hy", src).expect("parses");
    let go = emit_go(&ast).expect("emits");
    assert!(go.contains("defer func() { xs[0] = p }()"));
    assert!(go.contains("defer func() { ys[0] = q }()"));
}
