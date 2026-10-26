use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).unwrap_or_else(|d| panic!("parse failed: {:?}", d));
    check_file(&ast)
}

#[test]
fn type_alias_composite_literal_preserves_map_and_slice_shapes() {
    let r_freeze = check(
        "type str_map = map[string]int\n\
         fn worker(m str_map, done chan<- bool) { m[\"w\"] = 1\n done <- true }\n\
         fn main() {\n\
           done := make(chan bool, 1)\n\
           m := str_map{}\n\
           m[\"k\"] = 1\n\
           m2 := m\n\
           task worker(move m, done)\n\
           m2[\"k\"] = 2\n\
           <-done\n\
         }",
    );
    assert_eq!(r_freeze.verdict, Verdict::Reject, "{:?}", r_freeze.diagnostics);
    assert_eq!(r_freeze.diagnostics[0].code, "HYB-E3003");

    let r_non_owned = check(
        "type map_box = map[string]map[string]int\n\
         fn worker(m map[string]int, done chan<- bool) { m[\"w\"] = 1\n done <- true }\n\
         fn spawn_it(p map[string]int, done chan<- bool) {\n\
           mm := map_box{}\n\
           mm[\"a\"] = p\n\
           x := mm[\"a\"]\n\
           var m2 map[string]int = x\n\
           task worker(move m2, done)\n\
         }",
    );
    assert_eq!(
        r_non_owned.verdict,
        Verdict::Reject,
        "{:?}",
        r_non_owned.diagnostics
    );
    assert_eq!(r_non_owned.diagnostics[0].code, "HYB-E3004");

    let r_closure_map = check(
        "type fn_map = map[string]fn()\n\
         struct dummy {}\n\
         fn worker(fm fn_map) {}\n\
         fn main() {\n\
           fm := fn_map{}\n\
           task worker(move fm)\n\
         }",
    );
    assert_eq!(
        r_closure_map.verdict,
        Verdict::Reject,
        "{:?}",
        r_closure_map.diagnostics
    );
    assert_eq!(r_closure_map.diagnostics[0].code, "HYB-E3001");
}

#[test]
fn fn_field_index_and_alias_calls_preserve_return_shape() {
    let r_field = check(
        "struct holder { get fn() map[string]int }\n\
         fn worker(m map[string]int, done chan<- bool) { m[\"w\"] = 1\n done <- true }\n\
         fn main() {\n\
           done := make(chan bool, 1)\n\
           m := map[string]int{\"k\": 1}\n\
           h := holder{get: fn() map[string]int { return m }}\n\
           x := h.get()\n\
           var m2 map[string]int = x\n\
           task worker(move m2, done)\n\
           x[\"k\"] = 2\n\
           <-done\n\
         }",
    );
    assert_eq!(r_field.verdict, Verdict::Reject, "{:?}", r_field.diagnostics);
    assert_eq!(r_field.diagnostics[0].code, "HYB-E3003");

    let r_index = check(
        "fn worker(m map[string]int, done chan<- bool) { m[\"w\"] = 1\n done <- true }\n\
         fn main() {\n\
           done := make(chan bool, 1)\n\
           m := map[string]int{\"k\": 1}\n\
           gs := []fn() map[string]int{fn() map[string]int { return m }}\n\
           x := gs[0]()\n\
           var m2 map[string]int = x\n\
           task worker(move m2, done)\n\
           x[\"k\"] = 2\n\
           <-done\n\
         }",
    );
    assert_eq!(r_index.verdict, Verdict::Reject, "{:?}", r_index.diagnostics);
    assert_eq!(r_index.diagnostics[0].code, "HYB-E3003");
}

#[test]
fn untyped_slice_lit_starting_with_none_preserves_later_elem_shape() {
    let r = check(
        "fn worker(m option[map[string]int], done chan<- bool) { done <- true }\n\
         fn main() {\n\
           done := make(chan bool, 1)\n\
           m := map[string]int{\"k\": 1}\n\
           xs := [none, some(m)]\n\
           x := xs[1]\n\
           var m2 option[map[string]int] = x\n\
           task worker(move m2, done)\n\
           m[\"k\"] = 2\n\
           <-done\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(r.diagnostics[0].code, "HYB-E3003");
}
