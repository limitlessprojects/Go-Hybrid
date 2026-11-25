#[test]
fn binding_kind_of_ty_resolves_type_aliases_for_set_map_chan_and_foreign_3549() {
    let src = "import \"net/url\"\ntype int_set = set[int]\ntype sym_map = map[string]int\ntype int_chan = chan int\ntype my_url = url.url\nfn run(s int_set, m sym_map, ch int_chan, u *my_url) {\n _ = len(s)\n delete(s, 1)\n for v in s { _ = v }\n for k in m { _ = k }\n for x in ch { _ = x }\n u.raw_query = \"a=1\"\n}\n";
    let go = hy_emit::emit_go(&hy_syntax::parse_file("test.hy", src).unwrap()).unwrap();
    assert!(go.contains("s.len()") && go.contains("s.remove(1)") && go.contains("for v := range s.m") && go.contains("for k := range m") && go.contains("for x := range ch") && go.contains("u.RawQuery = \"a=1\""), "{go}");
}
