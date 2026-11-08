use hy_check::check_file;
use hy_syntax::parse_file;

#[test]
fn generic_fn_type_arg_paren_array_lit_and_move_root_ident_3019() {
    let r1 = check_file(&parse_file("t.hy", "struct box[t any] { cb t }\nfn take(b box[fn(c chan<- int)]) { _ = b }\nfn main() { var b box[fn(c <-chan int)]\n take(b) }").unwrap());
    assert!(r1.diagnostics.iter().any(|d| d.code == "HYB-E1002"), "{:?}", r1.diagnostics);
    let r2 = check_file(&parse_file("t.hy", "import \"net/http\"\nfn main() { a := ([1]http.handler{nil})\n fmt.println(len(a)) }").unwrap());
    assert!(!r2.diagnostics.iter().any(|d| d.code == "HYB-E4001"), "{:?}", r2.diagnostics);
    let r3 = check_file(&parse_file("t.hy", "import \"rust/serde_json\"\nfn run() error { doc := serde_json.new_object()?\n child := serde_json.new_object()?\n view := child.array(\"items\")?\n doc.set(\"child\", move child)?\n fmt.println(len(view))\n return nil }\nfn main() { _ = run() }").unwrap());
    assert!(r3.diagnostics.iter().any(|d| d.code == "HYB-E3101"), "{:?}", r3.diagnostics);
}
