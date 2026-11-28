//! Issue #4327: `parse_binary` must respect Go ASI-style newline boundaries so a
//! line-leading `-b`, `&b`, or `*b` starts a fresh unary expression statement
//! instead of silently folding across statements into `a - b` / `a & b` / `a * b`.

use hy_syntax::{parse_file, BinOp, Expr, Stmt, UnOp};

fn body(src: &str) -> Vec<Stmt> {
    let f = parse_file("t.hy", src).expect("parse");
    f.fns().last().unwrap().body.as_ref().unwrap().stmts.clone()
}

#[test]
fn line_leading_binary_ops_do_not_fold_across_newlines() {
    let stmts = body(
        "fn main() {\n\
             a := 10\n\
             b := 3\n\
             c := a\n\
             -b\n\
             d := a\n\
             &b\n\
         }\n",
    );
    assert_eq!(stmts.len(), 6, "got {stmts:?}");

    let Stmt::Local(c_decl) = &stmts[2] else {
        panic!("expected `c := a` local decl: {:?}", stmts[2]);
    };
    assert!(
        matches!(&c_decl.values[0], Expr::Ident { name, .. } if name == "a"),
        "`c := a` must not fold `-b` from next line: {:?}",
        c_decl.values[0]
    );
    assert!(
        matches!(
            &stmts[3],
            Stmt::Expr {
                expr: Expr::Unary { op: UnOp::Neg, .. },
                ..
            }
        ),
        "fourth statement must remain unary `-b`: {:?}",
        stmts[3]
    );

    let Stmt::Local(d_decl) = &stmts[4] else {
        panic!("expected `d := a` local decl: {:?}", stmts[4]);
    };
    assert!(
        matches!(&d_decl.values[0], Expr::Ident { name, .. } if name == "a"),
        "`d := a` must not fold `&b` from next line: {:?}",
        d_decl.values[0]
    );
    assert!(
        matches!(
            &stmts[5],
            Stmt::Expr {
                expr: Expr::AddrOf { .. },
                ..
            }
        ),
        "sixth statement must remain unary `&b`: {:?}",
        stmts[5]
    );
}

#[test]
fn trailing_binary_op_before_newline_continues_onto_next_line() {
    let stmts = body(
        "fn main() {\n\
             a := 10\n\
             b := 3\n\
             c := a +\n\
             b\n\
             d := a +\n\
             -b\n\
         }\n",
    );
    assert_eq!(stmts.len(), 4, "got {stmts:?}");

    let Stmt::Local(c_decl) = &stmts[2] else {
        panic!("expected `c := a + b` local decl: {:?}", stmts[2]);
    };
    assert!(
        matches!(&c_decl.values[0], Expr::Binary { op: BinOp::Add, .. }),
        "trailing `+` must continue onto next line: {:?}",
        c_decl.values[0]
    );

    let Stmt::Local(d_decl) = &stmts[3] else {
        panic!("expected `d := a + -b` local decl: {:?}", stmts[3]);
    };
    assert!(
        matches!(
            &d_decl.values[0],
            Expr::Binary { op: BinOp::Add, rhs, .. }
                if matches!(rhs.as_ref(), Expr::Unary { op: UnOp::Neg, .. })
        ),
        "trailing `+` followed by `-b` must parse as `a + (-b)`: {:?}",
        d_decl.values[0]
    );
}
