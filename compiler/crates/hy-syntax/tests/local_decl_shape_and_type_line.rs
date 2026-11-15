//! `let` / `var` / `const` declarations must carry the part their grammar
//! requires (SPEC-LANGUAGE §1.3), and a type annotation must start on the
//! declaration's own line (LX9: statements are newline-separated).
//!
//! Before this fix `hyb check` accepted every rejected program below with
//! `OK (0 diagnostics)`, and `hyb build` then failed (GUARANTEES Row 13):
//!   * `var x` + `fmt.println("{x}")` on the next line parsed as
//!     `var x fmt.println` + a bare parenthesized expression, emitted
//!     `var x fmt.Println` and `go build` refused it (`is not a type`);
//!   * `var x` + `x = 1` parsed as `var x x = 1` (Go: `x is not a type`);
//!   * `let x` / `let x int` / `const x` with no initializer reached the
//!     emitter, which refused them (`type-ascribed :=`, `const without an
//!     initializer`).

use hy_syntax::{parse_file, LocalKind, Stmt};

fn parse_err(src: &str) -> String {
    match parse_file("t.hy", src) {
        Ok(_) => panic!("must not parse:\n{src}"),
        Err(diags) => diags
            .iter()
            .map(|d| format!("{}: {}", d.code, d.message))
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn assert_rejects(src: &str, needle: &str) {
    let msg = parse_err(src);
    assert!(
        msg.contains("HYB-E0002") && msg.contains(needle),
        "expected HYB-E0002 mentioning `{needle}` for:\n{src}\ngot:\n{msg}"
    );
}

fn main_body(src: &str) -> Vec<Stmt> {
    let f = parse_file("t.hy", src).unwrap_or_else(|d| panic!("must parse:\n{src}\n{d:?}"));
    let body = f.fns().next().unwrap().body.as_ref().unwrap();
    body.stmts.clone()
}

#[test]
fn var_without_type_does_not_take_next_line_as_type() {
    // the next line is an ordinary statement, never the type of `x`
    assert_rejects(
        "fn main() {\n    var x\n    fmt.println(\"{x}\")\n}\n",
        "`var x` needs a type or an initializer",
    );
    assert_rejects(
        "fn main() {\n    var x\n    x = 1\n    fmt.println(\"{x}\")\n}\n",
        "`var x` needs a type or an initializer",
    );
    assert_rejects(
        "fn main() {\n    var x\n    *p = 1\n}\n",
        "`var x` needs a type or an initializer",
    );
    // at the end of a block too
    assert_rejects(
        "fn main() {\n    fmt.println(\"a\")\n    var x\n}\n",
        "`var x` needs a type or an initializer",
    );
}

#[test]
fn let_and_const_locals_need_an_initializer() {
    assert_rejects(
        "fn main() {\n    let x\n    fmt.println(\"{x}\")\n}\n",
        "`let x` needs an initializer",
    );
    assert_rejects(
        "fn main() {\n    let x int\n    fmt.println(\"{x}\")\n}\n",
        "`let x` needs an initializer",
    );
    assert_rejects(
        "fn main() {\n    const x\n    fmt.println(\"{x}\")\n}\n",
        "`const x` needs an initializer",
    );
    assert_rejects(
        "fn main() {\n    const x int\n}\n",
        "`const x` needs an initializer",
    );
}

#[test]
fn file_const_type_must_be_on_the_declaration_line() {
    // `const k` / `int = 3` used to read as `const k int = 3`
    let msg = parse_err("const k\nint = 3\nfn main() {\n    fmt.println(\"{k}\")\n}\n");
    assert!(msg.contains("HYB-E0002") && msg.contains("expected `=`"), "got:\n{msg}");
}

#[test]
fn the_error_is_reported_once_at_the_declaration() {
    // the declaration keeps its node, so recovery does not swallow the next
    // statement and cascade into more diagnostics
    let src = "fn main() {\n    var x\n    y := 2\n    fmt.println(\"{y}\")\n}\n";
    let Err(diags) = parse_file("t.hy", src) else { panic!("must not parse") };
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].span.start.line, 2);
    assert_eq!(diags[0].span.start.col, 5);
}

#[test]
fn well_formed_declarations_still_parse() {
    let stmts = main_body(
        "fn main() {\n\
         \x20   var a int\n\
         \x20   var b []int = [1]\n\
         \x20   var c = 3\n\
         \x20   var d map[string]int\n\
         \x20   var e *int\n\
         \x20   var f fn(int) int\n\
         \x20   let g = 1\n\
         \x20   let h := 2\n\
         \x20   const i = 4\n\
         \x20   const j int = 5\n\
         \x20   var k\n\
         \x20       = 6\n\
         }\n",
    );
    let shapes: Vec<(LocalKind, bool, usize)> = stmts
        .iter()
        .map(|s| match s {
            Stmt::Local(l) => (l.kind, l.ty.is_some(), l.values.len()),
            other => panic!("expected a local, got {other:?}"),
        })
        .collect();
    assert_eq!(
        shapes,
        vec![
            (LocalKind::Var, true, 0),
            (LocalKind::Var, true, 1),
            (LocalKind::Var, false, 1),
            (LocalKind::Var, true, 0),
            (LocalKind::Var, true, 0),
            (LocalKind::Var, true, 0),
            (LocalKind::Let, false, 1),
            (LocalKind::Let, false, 1),
            (LocalKind::Const, false, 1),
            (LocalKind::Const, true, 1),
            (LocalKind::Var, false, 1),
        ]
    );
}
