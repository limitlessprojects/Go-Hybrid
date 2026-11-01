use hy_check::check_file;
use hy_syntax::{parse_file, Decl, Stmt};

#[test]
fn fn_type_render_preserves_signature_for_generic_type_args_and_chan_dir_checks() {
    let ast = parse_file(
        "fn_render.hy",
        "struct box[t any] {\n\
           cb t\n\
         }\n\
         fn main() {\n\
           var b box[fn(c <-chan int) bool]\n\
         }\n",
    )
    .expect("parses generic struct with fn type arg");

    let f = ast.fns().next().expect("main fn");
    let body = f.body.as_ref().expect("body");
    match &body.stmts[0] {
        Stmt::Local(l) => {
            let rendered = l.ty.as_ref().expect("var type").render();
            assert_eq!(
                rendered, "box[fn(c <-chan int) bool]",
                "expected TypeExpr::Fn::render to preserve signature in generic type arg"
            );
        }
        other => panic!("expected local var decl, got {:?}", other),
    }

    let bad_src = "\
struct box[t any] {
  cb t
}
fn take_send_box(b box[fn(c chan<- int)]) {
}
fn main() {
  var b box[fn(c <-chan int)]
  take_send_box(b)
}
";
    let bad_ast = parse_file("bad_fn_box.hy", bad_src).expect("parses");
    let out = check_file(&bad_ast);
    assert!(
        out.diagnostics.iter().any(|d| d.code == "HYB-E3012"
            && d.message.contains("box[fn(c <-chan int)]")
            && d.message.contains("box[fn(c chan<- int)]")),
        "expected HYB-E3012 naming full generic fn types, got: {:?}",
        out.diagnostics
    );
}

#[test]
fn parenthesized_array_literal_peels_parens_in_as_array_lit_and_nil_check() {
    let src = "\
import \"net/http\"
fn main() {
  a := ([1]http.handler{nil})
  b := (([1]http.response_writer{nil}))
  c := ([2]int{1, 2})
  fmt.println(len(a), len(b), len(c))
}
";
    let ast = parse_file("paren_array_lit.hy", src).expect("parses");
    let f = ast.fns().next().expect("main fn");
    let body = f.body.as_ref().expect("body");
    for idx in 0..3 {
        match &body.stmts[idx] {
            Stmt::Local(l) => {
                assert!(
                    l.values[0].as_array_lit().is_some(),
                    "expected parenthesized array literal at stmt {idx} to resolve via as_array_lit()"
                );
            }
            other => panic!("expected local define, got {:?}", other),
        }
    }

    let out = check_file(&ast);
    assert!(
        out.diagnostics.is_empty(),
        "expected parenthesized array literals with nilable *int / chan int elements to pass without false HYB-E4001, got: {:?}",
        out.diagnostics
    );

    let bad_src = "\
fn main() {
  bad := ([1]int{nil})
  fmt.println(len(bad))
}
";
    let bad_ast = parse_file("bad_paren_array_lit.hy", bad_src).expect("parses");
    let bad_out = check_file(&bad_ast);
    assert!(
        bad_out.diagnostics.iter().any(|d| d.code == "HYB-E4001"
            && d.message.contains("the element slot `int`")),
        "expected HYB-E4001 to report element slot `int` (not `[1]int`), got: {:?}",
        bad_out.diagnostics
    );
}

#[test]
fn move_wrapped_access_chain_resolves_root_ident_and_enforces_a2_loan_conflict() {
    let ast = parse_file(
        "move_root.hy",
        "fn main() {\n\
           x := move s.items[0]\n\
         }\n",
    )
    .expect("parses");
    match &ast.decls[0] {
        Decl::Fn(f) => match &f.body.as_ref().unwrap().stmts[0] {
            Stmt::Local(l) => {
                assert_eq!(l.values[0].root_ident(), Some("s"));
            }
            other => panic!("expected local, got {:?}", other),
        },
        other => panic!("expected fn, got {:?}", other),
    }

    let a2_src = "\
import \"rust/serde_json\"
fn run() error {
  doc := serde_json.new_object()?
  child := serde_json.new_object()?
  view := child.array(\"items\")?
  doc.set(\"child\", move child)?
  n := len(view)
  fmt.println(n)
  return nil
}
";
    let a2_ast = parse_file("a2_move_consume.hy", a2_src).expect("parses");
    let out = check_file(&a2_ast);
    assert!(
        out.diagnostics.iter().any(|d| d.code == "HYB-E3101"),
        "expected HYB-E3101 A2 conflict when consuming `move child` while `view` loan is live, got: {:?}",
        out.diagnostics
    );
}
