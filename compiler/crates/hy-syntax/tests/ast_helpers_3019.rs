//! Regression tests for the cross-component AST helpers fixed in #3019
//! (`compiler/crates/hy-syntax/src/ast.rs`):
//!   * `TypeExpr::render()` for `TypeExpr::Fn` must keep the full signature,
//!     so a generic type argument survives the parser's
//!     render -> reparse round-trip (`parse_type` folds args via `render()`),
//!   * `Expr::as_array_lit()` must peel `Expr::Paren` (callers match the
//!     parenthesized value and then ask the still-`Paren` node for its shape),
//!   * `Expr::root_ident()` must see through `Expr::Move`.

use hy_syntax::{parse_file, Expr, Stmt};

fn first_fn_stmts(src: &str) -> Vec<Stmt> {
    let file = parse_file("t.hy", src).expect("source must parse");
    let f = file.fns().next().expect("expected one fn decl");
    f.body.as_ref().expect("fn has a body").stmts.clone()
}

#[test]
fn fn_type_render_keeps_its_signature_through_generic_args() {
    // `parse_type` folds a generic instantiation's arguments with
    // `TypeExpr::render()`; before the fix the `Fn` arm rendered a bare
    // `"fn"`, so `box[fn(c <-chan int) bool]` collapsed to `box[fn]` and
    // `hy-check`'s `parse_generic_type_args` could no longer reparse it.
    let stmts = first_fn_stmts("fn main() { var f box[fn(c <-chan int) bool] }");
    let Stmt::Local(l) = &stmts[0] else {
        panic!("expected a local, got {stmts:?}")
    };
    let ty = l.ty.as_ref().expect("`var` carries a type ascription");
    assert_eq!(ty.render(), "box[fn(c <-chan int) bool]");
}

#[test]
fn as_array_lit_sees_through_parentheses() {
    // `([1]int{nil})` — the paren re-enables the composite literal, but the
    // still-parenthesized node must still be recognized as an array literal
    // (else the element slot is read as the non-nilable array type).
    let stmts = first_fn_stmts("fn main() { a := ([1]int{nil}) }");
    let Stmt::Local(l) = &stmts[0] else {
        panic!("expected a local, got {stmts:?}")
    };
    let expr = &l.values[0];
    assert!(
        matches!(expr, Expr::Paren { .. }),
        "expected a parenthesized expression, got {expr:?}"
    );
    let (len, elem, _) = expr.as_array_lit().expect("parenthesized array literal");
    assert!(len.is_some(), "the array length is a literal");
    assert_eq!(elem.render(), "int");
}

#[test]
fn root_ident_sees_through_move() {
    // Consuming/moving boundary calls resolve their argument root;
    // `move child` denotes the same binding as `child`.
    let stmts = first_fn_stmts("fn main() {\n    child := 1\n    doc.set(\"k\", move child)\n}");
    let Stmt::Expr {
        expr: Expr::Call { args, .. },
        ..
    } = &stmts[1]
    else {
        panic!("expected a call, got {stmts:?}")
    };
    let mv = &args[1].value;
    assert!(
        matches!(mv, Expr::Move { .. }),
        "expected a move, got {mv:?}"
    );
    assert_eq!(mv.root_ident(), Some("child"));
    assert_eq!(mv.render(), "move child");
}
