//! Regression tests (accept controls) for #2418: A5 foreign Go-escape check-site (`HYB-E3013`).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn assert_accept(src: &str) {
    let ast = parse_file("test.hy", src).expect("parses");
    let res = check_file(&ast);
    assert_eq!(res.verdict, Verdict::Accept, "expected Accept, got {:?} for:\n{src}", res.diagnostics);
}

#[test]
fn read_only_bound_closure_and_blank_discard_accept() {
    assert_accept(
        "import \"net/http\"\nfn main() {\n    hits := 0\n    mux := http.new_serve_mux()\n    cb := fn(w http.response_writer, r *http.request) {\n        _ = w\n        _ = r\n        fmt.println(hits)\n    }\n    mux.handle_func(\"/x\", cb)\n}\n",
    );
}

#[test]
fn closure_local_map_mutation_and_local_shadow_before_write_accept() {
    assert_accept(
        "import \"net/http\"\nfn main() {\n    hits := 0\n    _ = hits\n    http.handle_func(\"/x\", fn(w http.response_writer, r *http.request) {\n        hits := 0\n        hits++\n        local_m := map[string]int{\"a\": hits}\n        delete(local_m, \"a\")\n        clear(local_m)\n    })\n}\n",
    );
}

#[test]
fn local_hybrid_struct_receiver_and_managed_interop_accept() {
    assert_accept(
        "struct router {}\nfn (rt router) handle_func(p string, f fn()) { f() }\nfn main() {\n    hits := 0\n    rt := router{}\n    rt.handle_func(\"/x\", fn() { hits++ })\n}\n",
    );
    assert_accept(
        "import \"net/http\"\nfn main() {\n    hits := 0\n    managed(\"atomic/synchronized interop\", \"interop\") {\n        http.handle_func(\"/x\", fn(w http.response_writer, r *http.request) { hits++ })\n    }\n}\n",
    );
}
