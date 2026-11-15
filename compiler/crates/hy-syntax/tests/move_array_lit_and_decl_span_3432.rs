use hy_syntax::{parse_file, Stmt};

#[test]
fn move_array_lit_and_decl_span_work() {
    let f = parse_file("t.hy", "type id = int\nfn main() {\n    x := move [2]int{1, 2}\n}\n").expect("parse");
    assert_eq!(f.decls[0].span().start.line, 1);
    assert_eq!(f.decls[1].span().start.line, 2);
    let body = f.fns().next().unwrap().body.as_ref().unwrap();
    let Stmt::Local(l) = &body.stmts[0] else { panic!("expected Local") };
    let (_, elem, arr_ty) = l.values[0].as_array_lit().expect("move [2]int{...} is array lit");
    assert_eq!(elem.render(), "int");
    assert_eq!(arr_ty.render(), "[2]int");
}
