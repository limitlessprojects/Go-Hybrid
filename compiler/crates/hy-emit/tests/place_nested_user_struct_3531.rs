#[test]
fn type_of_place_resolves_nested_user_struct_chains_3531() {
    let src = "import \"net/url\"\nimport \"net/http\"\nstruct inner_cfg { u *url.url\n req *http.request\n w http.response_writer }\nstruct outer_cfg { sub inner_cfg }\nfn update(o *outer_cfg) { o.sub.u.raw_query = \"a=1\"\n o.sub.req.url.path = \"/ok\"\n o.sub.w.header[\"k\"] = []string{\"v\"} }\n";
    let go = hy_emit::emit_go(&hy_syntax::parse_file("test.hy", src).unwrap()).unwrap();
    assert!(go.contains("o.sub.u.RawQuery = \"a=1\"") && go.contains("o.sub.req.URL.Path = \"/ok\"") && go.contains("o.sub.w.Header()[\"k\"] = []string{\"v\"}"), "{go}");
}
