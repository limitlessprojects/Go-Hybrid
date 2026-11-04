#[test]
fn type_of_expr_and_resolve_to_struct_name_follow_foreign_aliases_and_generic_structs_3664() {
    let src = "import \"net/http\"\ntype lock_t = sync.mutex\ntype req_t = *http.request\nstruct box[t] { mu lock_t\n req req_t\n val t }\nstruct outer { sub box[int] }\nfn run(o *outer) string {\n o.sub.mu.lock()\n (&box[int]{}).mu.unlock()\n return o.sub.req.url.query.get(\"id\")\n}\n";
    let go = hy_emit::emit_go(&hy_syntax::parse_file("test.hy", src).unwrap()).unwrap();
    assert!(go.contains("o.sub.mu.Lock()") && go.contains(".mu.Unlock()") && go.contains("o.sub.req.URL.Query().Get(\"id\")") && !go.contains(".mu.lock()"), "{go}");
}
