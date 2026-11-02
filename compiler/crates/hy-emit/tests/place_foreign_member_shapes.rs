use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("parse");
    emit_go(&ast).expect("emit_go")
}

#[test]
fn assignment_places_resolve_foreign_shapes_like_expressions() {
    // Row 13: the expression side consults SHAPES (MethodZero parens and
    // foreign fields nested in user structs). Assignment places used to
    // look only at the root binding, so `hyb check` accepted programs whose
    // emitted Go did not build.
    let go = emit(
        "struct req_ctx {\n\
         \treq *http.request\n\
         \tnote string\n\
         }\n\
         \n\
         fn route(mux *http.serve_mux) {\n\
         \tmux.handle(\"/x\", http.handler(fn(w, r) {\n\
         \t\tw.header[\"X-Mode\"] = []string{\"fast\"}\n\
         \t\tr.url.host = \"example.com\"\n\
         \t}))\n\
         }\n\
         \n\
         fn set_host(c *req_ctx) {\n\
         \tc.req.host = \"example.com\"\n\
         \tc.note = \"keep\"\n\
         }\n",
    );
    assert!(
        go.contains("w.Header()[\"X-Mode\"] = []string{\"fast\"}"),
        "MethodZero header must be called before indexing, got:\n{}",
        go
    );
    assert!(
        !go.contains("w.Header[\"X-Mode\"]"),
        "missing Header() call leaked into the assignment, got:\n{}",
        go
    );
    assert!(
        go.contains("r.URL.Host = \"example.com\""),
        "shape table spells URL exactly, got:\n{}",
        go
    );
    assert!(
        !go.contains("r.Url"),
        "naive M4 CamelCase leaked on the place, got:\n{}",
        go
    );
    assert!(
        go.contains("c.req.Host = \"example.com\""),
        "foreign field nested in a user struct must normalize, got:\n{}",
        go
    );
    assert!(
        !go.contains("c.req.host"),
        "lowercase nested foreign field leaked, got:\n{}",
        go
    );
    assert!(
        go.contains("c.note = \"keep\""),
        "our own struct fields stay verbatim, got:\n{}",
        go
    );
    assert!(
        !go.contains("c.Note"),
        "user field was renamed, got:\n{}",
        go
    );
}
