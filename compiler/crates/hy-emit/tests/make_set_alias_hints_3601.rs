#[test]
fn make_and_set_lit_resolve_type_aliases_3601() {
    let src = "type int_set = set[int]\ntype sym_map = map[string]int\ntype int_chan = chan int\nfn run() {\n ch := make(int_chan, 1)\n for x in ch { _ = x }\n for y in make(int_chan, 1) { _ = y }\n m := make(sym_map)\n for k in m { _ = k }\n for k2 in make(sym_map) { _ = k2 }\n _ = len(int_set{})\n for v in (int_set{}) { _ = v }\n}\n";
    let go = hy_emit::emit_go(&hy_syntax::parse_file("test.hy", src).unwrap()).unwrap();
    assert!(go.contains("for x := range ch") && go.contains("for y := range make(int_chan, 1)") && go.contains("for k := range m") && go.contains("for k2 := range make(sym_map)") && go.contains("int_set{}.len()") && go.contains("for v := range (int_set{}).m"), "{go}");
}
