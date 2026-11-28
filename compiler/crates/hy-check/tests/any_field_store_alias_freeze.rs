//! Regression tests for #4755: storing a reference-bearing value into an
//! `any`-typed struct field (or `[N]any` array slot) must materialize the O5.2
//! may-share edge so moving the container freezes the live alias (`HYB-E3003`),
//! while fresh literals and primitives remain accepted.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let file = parse_file("test.hy", src).expect("parse");
    check_file(&file)
}

#[test]
fn any_typed_field_store_freezes_shared_map_after_move() {
    let r = check(
        "struct holder {\n\
           m any\n\
         }\n\
         fn worker(h holder) {\n\
           h.m = h.m\n\
         }\n\
         fn main() {\n\
           shared := map[string]int{}\n\
           var h holder\n\
           h.m = shared\n\
           task worker(move h)\n\
           shared[\"main\"]++\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003, got {:?}",
        r.diagnostics
    );
}

#[test]
fn aliased_any_and_array_of_any_slots_also_freeze_shared_map() {
    let r_alias = check(
        "type erased = any\n\
         struct holder { m erased }\n\
         fn worker(h holder) { h.m = h.m }\n\
         fn main() {\n\
           shared := map[string]int{}\n\
           var h holder\n\
           h.m = shared\n\
           task worker(move h)\n\
           shared[\"k\"] = 1\n\
         }\n",
    );
    assert_eq!(r_alias.verdict, Verdict::Reject, "{:?}", r_alias.diagnostics);
    assert!(r_alias.diagnostics.iter().any(|d| d.code == "HYB-E3003"));

    let r_arr = check(
        "fn worker(a [1]any) { a[0] = a[0] }\n\
         fn main() {\n\
           shared := map[string]int{}\n\
           var a [1]any\n\
           a[0] = shared\n\
           task worker(move a)\n\
           shared[\"k\"] = 1\n\
         }\n",
    );
    assert_eq!(r_arr.verdict, Verdict::Reject, "{:?}", r_arr.diagnostics);
    assert!(r_arr.diagnostics.iter().any(|d| d.code == "HYB-E3003"));

    let r_copy = check(
        "struct holder { m any }\n\
         fn worker(h holder) { h.m = h.m }\n\
         fn main() {\n\
           shared := map[string]int{}\n\
           h1 := holder{m: shared}\n\
           h2 := h1\n\
           task worker(move h2)\n\
           shared[\"k\"] = 1\n\
         }\n",
    );
    assert_eq!(r_copy.verdict, Verdict::Reject, "{:?}", r_copy.diagnostics);
    assert!(r_copy.diagnostics.iter().any(|d| d.code == "HYB-E3003"));
}

#[test]
fn fresh_map_literal_and_primitive_in_any_field_remain_accepted() {
    let r_fresh = check(
        "struct holder { m any }\n\
         fn worker(h holder) { h.m = h.m }\n\
         fn main() {\n\
           var h holder\n\
           h.m = map[string]int{}\n\
           task worker(move h)\n\
         }\n",
    );
    assert_eq!(r_fresh.verdict, Verdict::Accept, "{:?}", r_fresh.diagnostics);

    let r_prim = check(
        "struct holder { m any }\n\
         fn worker(h holder) { h.m = h.m }\n\
         fn main() {\n\
           n := 42\n\
           var h holder\n\
           h.m = n\n\
           task worker(move h)\n\
           n++\n\
         }\n",
    );
    assert_eq!(r_prim.verdict, Verdict::Accept, "{:?}", r_prim.diagnostics);
}
