//! emit_stmt_expr_gaps.rs — regression tests for #1993: four constructs the
//! checker accepts but whose emission either failed or produced invalid Go.

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("test source must parse");
    emit_go(&ast).expect("test source must emit cleanly")
}

#[test]
fn parenthesized_address_of_peels_the_paren() {
    let go = emit(
        "struct point {\n    x int\n}\n\nfn main() {\n    pt := point{x: 1}\n    p := &(pt)\n    _ = p\n}\n",
    );
    assert!(
        go.contains("&pt"),
        "expected `&(pt)` to emit `&pt`:\n{go}"
    );
    assert!(
        !go.contains("&(pt)"),
        "invalid Go `&(pt)` leaked into the output:\n{go}"
    );
}

#[test]
fn elided_struct_map_key_inherits_key_type() {
    let go = emit(
        "struct point {\n    x int\n}\n\nfn main() {\n    m := map[point]int{{x: 2}: 9}\n    _ = m\n}\n",
    );
    assert!(
        go.contains("point{x: 2}: 9"),
        "expected the elided key to be spelled with the map's key type:\n{go}"
    );
}

#[test]
fn parenthesized_zero_result_try_is_a_statement() {
    // The report's failure was an emission error ("`?` outside a binding
    // position"), not a particular lowering. Parity with the unparenthesized
    // spelling is the contract.
    let bare = emit("fn do() error {\n    return nil\n}\n\nfn main() {\n    do()?\n}\n");
    let paren = emit("fn do() error {\n    return nil\n}\n\nfn main() {\n    (do()?)\n}\n");
    assert_eq!(
        paren, bare,
        "the parenthesized zero-result `?` must lower exactly like the bare one"
    );
}
