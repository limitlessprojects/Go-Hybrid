//! Regression tests (reject) for #2418: A5 foreign Go-escape check-site (`HYB-E3013`).

use hy_check::check_file;
use hy_syntax::parse_file;

fn assert_e3013(src: &str) {
    let codes: Vec<String> = check_file(&parse_file("test.hy", src).expect("parses"))
        .diagnostics.into_iter().map(|d| d.code).collect();
    assert!(codes.iter().any(|c| c == "HYB-E3013"), "expected HYB-E3013, got {codes:?} for:\n{src}");
}

#[test]
fn foreign_typed_chained_and_origin_receiver_calls_reject() {
    assert_e3013("import \"net/http\"\nfn register(mux *http.serve_mux) {\n    hits := 0\n    mux.handle_func(\"/x\", fn(w http.response_writer, r *http.request) { hits++ })\n}\nfn main() {}\n");
    assert_e3013("import \"net/http\"\nfn main() {\n    hits := 0\n    http.new_serve_mux().handle_func(\"/x\", fn(w http.response_writer, r *http.request) { hits++ })\n}\n");
    assert_e3013("import \"net/http\"\nfn main() {\n    hits := 0\n    mux := http.new_serve_mux()\n    mux.handle_func(\"/x\", fn(w http.response_writer, r *http.request) { hits++ })\n}\n");
}

#[test]
fn bound_closure_variable_passed_to_foreign_call_rejects() {
    assert_e3013("import \"time\"\nfn main() {\n    hits := 0\n    cb := fn() { hits++ }\n    _ = time.after_func(1, cb)\n}\n");
}

#[test]
fn nested_block_shadow_does_not_mask_captured_write() {
    assert_e3013("import \"net/http\"\nfn main() {\n    hits := 0\n    http.handle_func(\"/x\", fn(w http.response_writer, r *http.request) {\n        hits++\n        if false {\n            hits := 0\n            println(hits)\n        }\n    })\n}\n");
}

#[test]
fn captured_map_and_slice_builtin_mutations_reject() {
    assert_e3013("import \"net/http\"\nfn main() {\n    sessions := map[string]int{\"a\": 1}\n    http.handle_func(\"/a\", fn(w http.response_writer, r *http.request) { delete(sessions, \"a\") })\n}\n");
    assert_e3013("import \"net/http\"\nfn main() {\n    sessions := map[string]int{\"a\": 1}\n    http.handle_func(\"/b\", fn(w http.response_writer, r *http.request) { clear(sessions) })\n}\n");
    assert_e3013("import \"net/http\"\nfn main() {\n    buf := make([]byte, 4)\n    http.handle_func(\"/c\", fn(w http.response_writer, r *http.request) { _ = copy(buf, []byte(\"ok\")) })\n}\n");
}
