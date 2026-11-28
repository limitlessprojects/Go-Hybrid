//! Issue #3567: Go parses an `if` header with composite literals disabled, so
//! `if p := &t{a: 1}; cond {` is a Go syntax error. The emitter must
//! parenthesize an if-init value whose text has a top-level `{`.

fn emit(src: &str) -> String {
    let file = hy_syntax::parse_file("t.hy", src).expect("parse");
    hy_emit::emit_go(&file).unwrap_or_else(|e| panic!("emit failed: {}", e.message))
}

#[test]
fn if_init_addr_of_literal_is_parenthesized() {
    let go = emit("struct t { a int }\nfn main() {\n    if p := &t{a: 1}; p.a == 1 { println(\"ok\") }\n}\n");
    assert!(go.contains("if p := (&t{a: 1}); p.a == 1 {"), "got:\n{}", go);
}

#[test]
fn if_init_value_literal_is_parenthesized() {
    let go = emit("struct t { a int }\nfn main() {\n    if v := t{a: 1}; v.a == 1 { println(\"ok\") }\n}\n");
    assert!(go.contains("if v := (t{a: 1}); v.a == 1 {"), "got:\n{}", go);
}

#[test]
fn else_if_init_literal_is_parenthesized() {
    let go = emit("struct t { a int }\nfn main() {\n    x := 2\n    if x == 1 { println(\"a\") } else if v := t{a: x}; v.a == 2 { println(\"b\") }\n}\n");
    assert!(go.contains("else if v := (t{a: x}); v.a == 2 {"), "got:\n{}", go);
}

#[test]
fn if_init_plain_values_and_calls_stay_unparenthesized() {
    let go = emit("fn f() int { return 1 }\nfn main() {\n    if v := f(); v == 1 { println(\"ok\") }\n    if w := []int{1}; len(w) == 1 { println(\"ok\") }\n}\n");
    assert!(go.contains("if v := f(); v == 1 {"), "got:\n{}", go);
    assert!(go.contains("if w := ([]int{1}); len(w) == 1 {"), "got:
{}", go);
}
