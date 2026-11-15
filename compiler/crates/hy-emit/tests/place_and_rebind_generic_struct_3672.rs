#[test]
fn place_member_name_and_rebindings_resolve_through_generic_structs_3672() {
    let src = "import \"net/url\"\ntype my_url = url.url\nstruct box[t] { u my_url\n ch chan int\n val t }\nstruct holder { sub box[int] }\nfn run(h *holder) {\n h.sub.u.raw_query = \"a=1\"\n u := h.sub.u\n u.raw_query = \"b=2\"\n for v in h.sub.ch { _ = v }\n}\n";
    let go = hy_emit::emit_go(&hy_syntax::parse_file("test.hy", src).unwrap()).unwrap();
    assert!(go.contains("h.sub.u.RawQuery = \"a=1\"") && go.contains("u.RawQuery = \"b=2\"") && go.contains("for v := range h.sub.ch"), "{go}");
}
