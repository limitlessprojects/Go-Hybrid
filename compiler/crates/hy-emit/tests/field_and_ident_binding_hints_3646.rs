#[test]
fn struct_fields_and_ident_rebindings_preserve_set_map_and_chan_hints_3646() {
    let go = hy_emit::emit_go(&hy_syntax::parse_file("test.hy", "type int_set = set[int]\ntype sym_map = map[string]int\ntype int_chan = chan int\nstruct bag { seen int_set\n counts sym_map\n ch int_chan }\nfn run(b bag) {\n _ = len(b.seen)\n for v in b.seen { _ = v }\n for k in b.counts { _ = k }\n for x in b.ch { _ = x }\n s := b.seen\n s2 := s\n _ = len(s2)\n}\n").unwrap()).unwrap();
    assert!(go.contains("b.seen.len()") && go.contains("for v := range b.seen.m") && go.contains("for k := range b.counts") && go.contains("for x := range b.ch") && go.contains("s2.len()"), "{go}");
}
