//! Issue #1615: `new((T))` must get the same SPEC N5 allocation as `new(T)`.
  
use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn n5_new_peels_parenthesized_type_operand() {
    let src = "type counts = map[string]int\n\nstruct bag {\n    items map[string]int\n}\n\nfn main() {\n    b := new((bag))\n    b.items[\"k\"] = 1\n    c := new(((counts)))\n    fmt.println(b, c)\n}\n";
    let go = emit_go(&parse_file("test.hy", src).expect("parses")).expect("emits");
    assert!(go.contains("b := &bag{items: map[string]int{}}"), "{go}");
    assert!(go.contains("c := &counts{}"), "{go}");
}
