use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn closure_local_slot_loan_flushes_inside_closure_body_issue_202() {
    let src = "\
fn main() {
    m := map[string]int{\"k\": 0}
    f := fn() {
        slot := &m[\"k\"]
        slot = 42
    }
    f()
    v := m[\"k\"]
    fmt.println(\"val: {v}\")
}
";
    let ast = parse_file("closure_loan.hy", src).expect("parses");
    let go = emit_go(&ast).expect("emits");
    // The store-back `m["k"] = slot` must appear inside `f := func() { ... }`,
    // BEFORE `f()`, and must not be hoisted to the end of `main`.
    let fnlit_pos = go.find("f := func()").expect("has closure");
    let call_pos = go.find("\tf()\n").expect("has call");
    let store_pos = go.find("m[\"k\"] = slot").expect("has store-back");
    assert!(
        store_pos > fnlit_pos && store_pos < call_pos,
        "expected store-back inside closure before f(), got:\n{}",
        go
    );
    assert_eq!(
        go.matches("m[\"k\"] = slot").count(),
        1,
        "expected exactly 1 store-back, got:\n{}",
        go
    );
}
