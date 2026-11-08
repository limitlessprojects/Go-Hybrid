#[test]
fn place_member_name_and_local_rebindings_resolve_struct_and_foreign_aliases_3655() {
    let src = "import \"net/url\"\ntype my_url = url.url\nstruct leaf { u my_url\n mu sync.mutex }\ntype wrap = leaf\nstruct holder { sub wrap }\nfn run(h holder) {\n h.sub.u.raw_query = \"a=1\"\n s := h.sub\n s.u.raw_query = \"b=2\"\n s.mu.lock()\n}\n";
    let go = hy_emit::emit_go(&hy_syntax::parse_file("test.hy", src).unwrap()).unwrap();
    assert!(go.contains("h.sub.u.RawQuery = \"a=1\"") && go.contains("s.u.RawQuery = \"b=2\"") && go.contains("s.mu.Lock()"), "{go}");
}
