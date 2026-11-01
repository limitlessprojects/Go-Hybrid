//! Regression tests for foreign-escape closure checks (`HYB-E3013`) and
//! shadowed builtin argument checks (`HYB-E2005`) in `builtins.rs`.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn assert_rejects_code(src: &str, want: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected Reject with {want}, got: {:?}\n{src}",
        r.diagnostics
    );
    assert!(
        r.diagnostics.iter().any(|d| d.code == want),
        "expected {want}, got: {:?}\n{src}",
        r.diagnostics
    );
}

#[test]
fn move_wrapped_closure_arg_passed_to_foreign_call_rejects_e3013() {
    // `move cb` and `move fn(...) { ... }` passed to foreign calls must unwrap
    // `Expr::Move` and reject captured mutations with HYB-E3013.
    assert_rejects_code(
        "import \"net/http\"\n\
         fn main() {\n\
           hits := 0\n\
           cb := fn(w http.response_writer, r *http.request) { hits++ }\n\
           mux := http.new_serve_mux()\n\
           mux.handle_func(\"/a\", move cb)\n\
         }",
        "HYB-E3013",
    );
    assert_rejects_code(
        "import \"net/http\"\n\
         fn main() {\n\
           hits := 0\n\
           http.handle_func(\"/a\", move fn(w http.response_writer, r *http.request) { hits++ })\n\
         }",
        "HYB-E3013",
    );
}

#[test]
fn slice_alias_index_field_and_captured_struct_ref_field_alias_reject_e3013() {
    // Mutating a struct field through an indexed slice alias (`a := items; a[0].v = 2`)
    // mutates backing storage of captured `items` and must trigger HYB-E3013.
    assert_rejects_code(
        "import \"net/http\"\n\
         struct item { v int }\n\
         fn main() {\n\
           items := []item{item{v: 1}}\n\
           mux := http.new_serve_mux()\n\
           mux.handle_func(\"/a\", fn(w http.response_writer, r *http.request) {\n\
             a := items\n\
             a[0].v = 2\n\
           })\n\
         }",
        "HYB-E3013",
    );
    // Aliasing a pointer field or slice field of a captured struct (`p := h.ptr; p.v = 2`,
    // `s := h.items; s[0] = 2`) mutates shared state of captured `h` and must trigger HYB-E3013.
    assert_rejects_code(
        "import \"net/http\"\n\
         struct item { v int }\n\
         struct holder { ptr *item\n items []int }\n\
         fn main() {\n\
           it := item{v: 1}\n\
           h := holder{ptr: &it, items: []int{1}}\n\
           mux := http.new_serve_mux()\n\
           mux.handle_func(\"/a\", fn(w http.response_writer, r *http.request) {\n\
             p := h.ptr\n\
             p.v = 2\n\
           })\n\
         }",
        "HYB-E3013",
    );
    assert_rejects_code(
        "import \"net/http\"\n\
         struct holder { items []int }\n\
         fn main() {\n\
           h := holder{items: []int{1}}\n\
           mux := http.new_serve_mux()\n\
           mux.handle_func(\"/a\", fn(w http.response_writer, r *http.request) {\n\
             s := h.items\n\
             s[0] = 2\n\
           })\n\
         }",
        "HYB-E3013",
    );
}

#[test]
fn nested_closure_foreign_escape_and_shadowed_new_make_validate_properly() {
    // Inside an outer closure, local foreign receivers (`mux2 := http.new_serve_mux()`)
    // and local closure bindings (`cb2 := fn(...) { hits++ }`) must be tracked so
    // `mux2.handle_func("/c", cb2)` triggers HYB-E3013.
    assert_rejects_code(
        "import \"net/http\"\n\
         fn main() {\n\
           hits := 0\n\
           outer := fn() {\n\
             mux2 := http.new_serve_mux()\n\
             cb2 := fn(w http.response_writer, r *http.request) { hits++ }\n\
             mux2.handle_func(\"/c\", cb2)\n\
           }\n\
           outer()\n\
         }",
        "HYB-E3013",
    );
    // When `new` or `make` is shadowed by a local binding, its first argument is a
    // value expression (not a type argument) and must still be checked for HYB-E2005.
    assert_rejects_code(
        "fn main() {\n\
           new := fn(x int) { _ = x }\n\
           new(len(1, 2))\n\
         }",
        "HYB-E2005",
    );
}
