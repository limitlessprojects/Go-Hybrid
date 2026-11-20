//! Regression tests for foreign-escape closure checks (`HYB-E3013`) and
//! shadowed builtin argument checks (`HYB-E2005`) in `builtins.rs`.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn assert_rejects_code(src: &str, want: &str) {
    let r = check_file(&parse_file("test.hy", src).expect("parse"));
    assert!(r.verdict == Verdict::Reject && r.diagnostics.iter().any(|d| d.code == want), "{:?}", r.diagnostics);
}

#[test]
fn move_wrapped_closure_arg_passed_to_foreign_call_rejects_e3013() {
    for src in [
        "import \"net/http\"\nfn main() {\nhits := 0\ncb := fn(w http.response_writer, r *http.request) { hits++ }\nmux := http.new_serve_mux()\nmux.handle_func(\"/a\", move cb)\n}",
        "import \"net/http\"\nfn main() {\nhits := 0\nhttp.handle_func(\"/a\", move fn(w http.response_writer, r *http.request) { hits++ })\n}",
    ] {
        assert_rejects_code(src, "HYB-E3013");
    }
}

#[test]
fn slice_alias_index_field_and_captured_struct_ref_field_alias_reject_e3013() {
    for src in [
        "import \"net/http\"\nstruct item { v int }\nfn main() {\nitems := []item{item{v: 1}}\nmux := http.new_serve_mux()\nmux.handle_func(\"/a\", fn(w http.response_writer, r *http.request) {\na := items\na[0].v = 2\n})\n}",
        "import \"net/http\"\nstruct item { v int }\nstruct holder { ptr *item\n items []int }\nfn main() {\nit := item{v: 1}\nh := holder{ptr: &it, items: []int{1}}\nmux := http.new_serve_mux()\nmux.handle_func(\"/a\", fn(w http.response_writer, r *http.request) {\np := h.ptr\np.v = 2\n})\n}",
        "import \"net/http\"\nstruct holder { items []int }\nfn main() {\nh := holder{items: []int{1}}\nmux := http.new_serve_mux()\nmux.handle_func(\"/a\", fn(w http.response_writer, r *http.request) {\ns := h.items\ns[0] = 2\n})\n}",
    ] {
        assert_rejects_code(src, "HYB-E3013");
    }
}

#[test]
fn nested_closure_foreign_escape_and_shadowed_new_make_validate_properly() {
    assert_rejects_code("import \"net/http\"\nfn main() {\nhits := 0\nouter := fn() {\nmux2 := http.new_serve_mux()\ncb2 := fn(w http.response_writer, r *http.request) { hits++ }\nmux2.handle_func(\"/c\", cb2)\n}\nouter()\n}", "HYB-E3013");
    assert_rejects_code("fn main() {\nnew := fn(x int) { _ = x }\nnew(len(1, 2))\n}", "HYB-E2005");
}
