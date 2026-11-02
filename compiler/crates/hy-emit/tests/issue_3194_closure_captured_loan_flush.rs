//! issue_3194_closure_captured_loan_flush.rs — tests verifying that a block-scoped
//! slot loan captured by a closure is flushed AFTER the closure is called, not before.

fn emit(src: &str) -> String {
    let file = hy_syntax::parse_file("t.hy", src).expect("parse");
    hy_emit::emit_go(&file).unwrap_or_else(|e| panic!("emit failed: {}", e.message))
}

#[test]
fn for_body_closure_captured_loan_flushes_after_call() {
    let src = r#"
struct cell { n int }
fn main() {
    xs := []cell{cell{n: 1}, cell{n: 1}}
    for i in 0..2 {
        p := &xs[i]
        bump := fn() { p.n += 10 }
        bump()
    }
}
"#;
    let go = emit(src);
    let bump_call = go.find("bump()").expect("has bump() call");
    let store_back = go.find("xs[i] = p").expect("has store-back");
    assert!(
        bump_call < store_back,
        "store-back must flush AFTER closure call bump(), got:\n{go}"
    );
}

#[test]
fn if_body_closure_captured_loan_flushes_after_call() {
    let src = r#"
struct cell { n int }
fn main() {
    xs := []cell{cell{n: 1}}
    if true {
        p := &xs[0]
        bump := fn() { p.n = 7 }
        bump()
    }
}
"#;
    let go = emit(src);
    let bump_call = go.find("bump()").expect("has bump() call");
    let store_back = go.find("xs[0] = p").expect("has store-back");
    assert!(
        bump_call < store_back,
        "store-back must flush AFTER closure call bump(), got:\n{go}"
    );
}

#[test]
fn bare_block_slice_elem_closure_captured_loan_flushes_after_call() {
    let src = r#"
struct cell { n int }
fn main() {
    xs := []cell{cell{n: 1}}
    {
        p := &xs[0]
        f := fn() { p.n = 2 }
        f()
    }
}
"#;
    let go = emit(src);
    let f_call = go.find("f()").expect("has f() call");
    let store_back = go.find("xs[0] = p").expect("has store-back");
    assert!(
        f_call < store_back,
        "store-back must flush AFTER closure call f(), got:\n{go}"
    );
}

#[test]
fn bare_block_map_slot_closure_captured_loan_flushes_after_call() {
    let src = r#"
struct cell { n int }
fn main() {
    m := map[string]cell{"a": cell{n: 1}}
    {
        p := &m["a"]
        f := fn() { p.n = 2 }
        f()
    }
}
"#;
    let go = emit(src);
    let f_call = go.find("f()").expect("has f() call");
    let store_back = go.find("m[\"a\"] = p").expect("has store-back");
    assert!(
        f_call < store_back,
        "store-back must flush AFTER closure call f(), got:\n{go}"
    );
}
