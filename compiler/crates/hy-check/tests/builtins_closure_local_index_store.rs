//! Regression tests for `compiler/crates/hy-check/src/builtins.rs`:
//! `closure_writes_captured` must taint closure-local containers (`alias.is_none()`)
//! when a captured shared value is stored via `PlaceKind::Index` (`rows[0] = shared`,
//! `m["k"] = shared`, `arr[0] = shared`, `box.rows[0] = shared`) and subsequently mutated (`HYB-E3013`).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("builtins_closure_local_index_store.hy", src).expect("snippet parses");
    check_file(&ast)
}

#[test]
fn closure_local_slice_and_map_index_store_of_captured_shared_rejects_e3013() {
    for src in [
        "import \"net/http\"\n\
         fn main() {\n\
           shared := []int{1, 2}\n\
           mux := http.new_serve_mux()\n\
           mux.handle_func(\"/a\", fn(w http.response_writer, r *http.request) {\n\
             rows := [][]int{[]int{0}}\n\
             rows[0] = shared\n\
             rows[0][0] = 99\n\
           })\n\
         }",
        "import \"net/http\"\n\
         fn main() {\n\
           shared := map[string]int{\"k\": 1}\n\
           mux := http.new_serve_mux()\n\
           mux.handle_func(\"/a\", fn(w http.response_writer, r *http.request) {\n\
             nested := map[string]map[string]int{}\n\
             nested[\"m\"] = shared\n\
             delete(nested[\"m\"], \"k\")\n\
           })\n\
         }",
        "import \"net/http\"\n\
         struct item { v int }\n\
         fn main() {\n\
           it := item{v: 1}\n\
           mux := http.new_serve_mux()\n\
           mux.handle_func(\"/a\", fn(w http.response_writer, r *http.request) {\n\
             var ptrs [1]*item\n\
             ptrs[0] = &it\n\
             ptrs[0].v = 99\n\
           })\n\
         }",
        "import \"net/http\"\n\
         struct box { rows [][]int }\n\
         fn main() {\n\
           shared := []int{1, 2}\n\
           mux := http.new_serve_mux()\n\
           mux.handle_func(\"/a\", fn(w http.response_writer, r *http.request) {\n\
             b := box{rows: [][]int{[]int{0}}}\n\
             b.rows[0] = shared\n\
             b.rows[0][0] = 99\n\
           })\n\
         }",
    ] {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected Reject for:\n{}\ngot: {:?}",
            src,
            r.diagnostics
        );
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E3013"),
            "expected HYB-E3013 for:\n{}\ngot: {:?}",
            src,
            r.diagnostics
        );
    }
}

#[test]
fn positive_control_closure_local_index_store_of_fresh_value_accepts() {
    let r = check(
        "import \"net/http\"\n\
         fn main() {\n\
           mux := http.new_serve_mux()\n\
           mux.handle_func(\"/a\", fn(w http.response_writer, r *http.request) {\n\
             rows := [][]int{[]int{0}}\n\
             rows[0] = []int{1, 2}\n\
             rows[0][0] = 99\n\
           })\n\
         }",
    );
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected Accept, got {:?}",
        r.diagnostics
    );
}
