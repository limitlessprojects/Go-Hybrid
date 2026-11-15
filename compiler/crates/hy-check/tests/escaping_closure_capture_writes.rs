//! Soundness regression suite for A5 / SPEC-LANGUAGE §8.5 C9 (`HYB-E3013`):
//! closures that write or mutate captured bindings and escape into foreign calls.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn assert_rejects_e3013(src: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject, got: {:?}\n{src}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3013"),
        "expected HYB-E3013, got: {:?}\n{src}",
        r.diagnostics
    );
}

#[test]
fn named_closure_binding_and_alias_passed_to_foreign_call_reject_e3013() {
    assert_rejects_e3013(
        "import \"net/http\"\n\
         fn main() {\n\
           hits := 0\n\
           cb := fn() { hits++ }\n\
           http.handle_func(\"/x\", cb)\n\
         }",
    );
    assert_rejects_e3013(
        "import \"net/http\"\n\
         fn main() {\n\
           hits := 0\n\
           cb := fn() { hits += 1 }\n\
           cb2 := cb\n\
           mux := http.new_serve_mux()\n\
           mux.handle(\"/x\", http.handler(cb2))\n\
         }",
    );
}

#[test]
fn foreign_opaque_receiver_method_call_with_escaping_closure_rejects_e3013() {
    assert_rejects_e3013(
        "import \"net/http\"\n\
         fn main() {\n\
           hits := 0\n\
           mux := http.new_serve_mux()\n\
           mux.handle_func(\"/x\", fn() { hits++ })\n\
         }",
    );
    assert_rejects_e3013(
        "import \"net/http\"\n\
         fn main() {\n\
           hits := 0\n\
           mux := http.new_serve_mux()\n\
           cb := fn() { hits = 1 }\n\
           mux.handle_func(\"/x\", cb)\n\
         }",
    );
}

#[test]
fn builtin_method_and_alias_mutations_in_escaping_closure_reject_e3013() {
    for body in [
        "m := map[string]int{\"k\": 1}\nmux.handle(\"/x\", http.handler(fn() { delete(m, \"k\") }))",
        "m := map[string]int{\"k\": 1}\nmux.handle(\"/x\", http.handler(fn() { clear(m) }))",
        "dst := []int{0, 0}\nmux.handle(\"/x\", http.handler(fn() { _ = copy(dst, []int{1, 2}) }))",
        "s := set[string]{}\nmux.handle(\"/x\", http.handler(fn() { _ = s.insert(\"a\") }))",
        "arr := []int{3, 1, 2}\nmux.handle(\"/x\", http.handler(fn() { sort.ints(arr) }))",
        "m := map[string]int{\"k\": 1}\nmux.handle(\"/x\", http.handler(fn() { a := m\na[\"k\"] = 2 }))",
    ] {
        assert_rejects_e3013(&format!(
            "import \"net/http\"\nimport \"sort\"\nfn main() {{\n  mux := http.new_serve_mux()\n  {body}\n}}"
        ));
    }
    assert_rejects_e3013(
        "import \"net/http\"\n\
         struct item { v int }\n\
         fn main() {\n\
           m := map[string]item{\"k\": item{v: 1}}\n\
           mux := http.new_serve_mux()\n\
           mux.handle(\"/x\", http.handler(fn() { p := &m[\"k\"]\np.v = 2 }))\n\
         }",
    );
}

#[test]
fn inner_block_shadow_does_not_mask_outer_captured_write_e3013() {
    assert_rejects_e3013(
        "import \"net/http\"\n\
         fn main() {\n\
           hits := 0\n\
           mux := http.new_serve_mux()\n\
           mux.handle(\"/x\", http.handler(fn() {\n\
             hits = hits + 1\n\
             if false { hits := 0\n_ = hits }\n\
           }))\n\
         }",
    );
    assert_rejects_e3013(
        "import \"net/http\"\n\
         fn main() {\n\
           hits := 0\n\
           mux := http.new_serve_mux()\n\
           mux.handle(\"/x\", http.handler(fn() {\n\
             if true { hits := 0\nhits++\n_ = hits }\n\
             hits = 1\n\
           }))\n\
         }",
    );
    let ok = check(
        "import \"net/http\"\n\
         fn main() {\n\
           hits := 0\n\
           _ = hits\n\
           mux := http.new_serve_mux()\n\
           mux.handle(\"/x\", http.handler(fn() {\n\
             if true { hits := 0\nhits++\n_ = hits }\n\
           }))\n\
         }",
    );
    assert_eq!(ok.verdict, Verdict::Accept, "{:?}", ok.diagnostics);
}
