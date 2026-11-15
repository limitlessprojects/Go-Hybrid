//! SPEC §1.3 `PanicStmt = "panic" Expression`. Only a string-literal operand
//! was parsed as the prefix form; `panic msg` split into two expression
//! statements (`panic` and `msg`), passed `hyb check`, and emitted Go that
//! `go build` refuses with "panic (built-in) must be called".

use hy_syntax::{parse_file, Expr, Stmt};

fn body(src: &str) -> Vec<Stmt> {
    let f = parse_file("t.hy", src).expect("parse");
    f.fns().last().unwrap().body.as_ref().unwrap().stmts.clone()
}

/// The statement is one `panic(<operand>)` call; returns the operand.
fn panic_operand(stmt: &Stmt) -> &Expr {
    let Stmt::Expr { expr: Expr::Call { callee, args, .. }, .. } = stmt else {
        panic!("expected a panic call statement, got {:?}", stmt)
    };
    assert!(matches!(callee.as_ref(), Expr::Ident { name, .. } if name == "panic"));
    assert_eq!(args.len(), 1, "panic takes exactly one operand");
    &args[0].value
}

#[test]
fn panic_prefix_with_identifier_operand_is_one_statement() {
    let stmts = body("fn main() {\n    msg := \"boom\"\n    panic msg\n}\n");
    assert_eq!(stmts.len(), 2, "`panic msg` must not split into two statements");
    assert!(matches!(panic_operand(&stmts[1]), Expr::Ident { name, .. } if name == "msg"));
}

#[test]
fn panic_prefix_takes_the_whole_expression() {
    for src in [
        "fn main() {\n    a := \"x\"\n    panic a + \"y\"\n}\n",
        "fn msg() string { return \"m\" }\nfn main() {\n    a := 0\n    panic msg()\n}\n",
        "fn main() {\n    a := 0\n    panic 42\n}\n",
        "fn main() {\n    a := 0\n    panic 'x'\n}\n",
        "fn main() {\n    a := 0\n    panic 1.5\n}\n",
        "fn main() {\n    a := 0\n    panic true\n}\n",
        "fn main() {\n    a := true\n    panic !a\n}\n",
    ] {
        let stmts = body(src);
        assert_eq!(stmts.len(), 2, "split panic statement in:\n{src}");
        panic_operand(&stmts[1]);
    }
}

#[test]
fn panic_string_literal_and_call_forms_are_unchanged() {
    let stmts = body("fn main() {\n    panic \"boom\"\n}\n");
    assert_eq!(stmts.len(), 1);
    panic_operand(&stmts[0]);
    let stmts = body("fn main() {\n    panic(\"boom\")\n}\n");
    assert_eq!(stmts.len(), 1);
    panic_operand(&stmts[0]);
}

#[test]
fn identifier_on_the_next_line_is_a_separate_statement() {
    // A user binding named `panic` used as a bare expression statement,
    // followed by an unrelated statement on the next line, keeps its two
    // statements: the prefix form never reaches across a newline.
    let stmts = body("fn main() {\n    panic := 1\n    panic\n    x := 2\n    _ = x\n}\n");
    assert_eq!(stmts.len(), 4);
    assert!(matches!(&stmts[1], Stmt::Expr { expr: Expr::Ident { name, .. }, .. } if name == "panic"));
}
