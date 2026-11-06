//! T24 / O8 (`HYB-E3104`) soundness guards in `addr_loan.rs`: `PlaceKind::AddrOf` targets,
//! mixed-target parallel index rebinds, `move`-wrapped `&(...)`, and loop-carried slot loans.
use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn assert_e3104(src: &str) {
    let r = check_file(&parse_file("t.hy", src).expect("snippet parses"));
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{src}\ngot: {:?}", r.diagnostics);
    let e3104 = r.diagnostics.iter().filter(|d| d.code == "HYB-E3104").count();
    assert_eq!(e3104, 1, "expected exactly 1 HYB-E3104, got {:?} for:\n{src}", r.diagnostics);
}

fn assert_accept(src: &str) {
    let r = check_file(&parse_file("t.hy", src).expect("snippet parses"));
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept, got {:?} for:\n{src}", r.diagnostics);
    assert!(r.diagnostics.is_empty(), "expected 0 diagnostics, got {:?}", r.diagnostics);
}

const PRE: &str = "struct entry {\n    val int\n    arr [2]int\n}\nstruct tag_box { tag int }\nfn consume_ptr(p *entry) { _ = p.val }\nfn check_ptr(p *entry) bool { return p.val > 0 }\nfn check_int_ptr(p *int) bool { return p != nil }";

#[test]
fn place_addrof_and_mixed_parallel_index_rebind_reject_e3104() {
    for body in [
        "m := map[string]entry{\"k\": entry{val: 1, arr: [2]int{0, 0}}}\nvar q entry\nq = &m[\"k\"]\nother := entry{val: 2, arr: [2]int{0, 0}}\nif true {\n    &q = &other\n}\n_ = q.val",
        "m := map[string]entry{\"k\": entry{val: 1, arr: [2]int{0, 0}}}\nq := &m[\"k\"]\nx := 9\nif true {\n    &q.val = &x\n}\n_ = q.val",
        "m := map[string]entry{\"k\": entry{val: 1, arr: [2]int{0, 0}}}\nq := &m[\"k\"]\nx := 9\nif true {\n    &q.arr[0] = &x\n}\n_ = q.val",
        "m := map[string]entry{\"k\": entry{val: 1, arr: [2]int{0, 0}}}\nother := entry{val: 2, arr: [2]int{0, 0}}\nif true {\n    &m[\"k\"] = &other\n}",
        "m := map[string]entry{\"k\": entry{val: 1, arr: [2]int{0, 0}}}\nx := 9\nif true {\n    &m[\"k\"].val = &x\n}",
        "m := map[string]entry{\"k\": entry{val: 1, arr: [2]int{0, 0}}}\nxs := []entry{entry{val: 5, arr: [2]int{0, 0}}}\nq := &m[\"k\"]\nsink := []int{0}\nsink[0], q = 1, &xs[0]\n_ = q.val",
        "m := map[string]entry{\"k\": entry{val: 1, arr: [2]int{0, 0}}}\narr := [1]entry{entry{val: 5, arr: [2]int{0, 0}}}\nq := &m[\"k\"]\nbx := tag_box{tag: 0}\nbx.tag, q = 1, &arr[0]\n_ = q.val",
        "m := map[string]entry{\"k\": entry{val: 1, arr: [2]int{0, 0}}}\np := &(move m[\"k\"])\n_ = p",
        "m := map[string]entry{\"k\": entry{val: 1, arr: [2]int{0, 0}}}\n_ = check_int_ptr(&(move m[\"k\"].val))",
        "m := map[string]entry{\"k\": entry{val: 1, arr: [2]int{0, 0}}}\nq := &m[\"k\"]\np := &(move q.val)\n_ = p",
        "f := fn() {\n    m := map[string]entry{\"k\": entry{val: 1, arr: [2]int{0, 0}}}\n    q := &m[\"k\"]\n    p := &(move q)\n    _ = p\n}\nf()",
    ] {
        assert_e3104(&format!("{PRE}\nfn main() {{\n{body}\n}}\n"));
    }
}

#[test]
fn loop_carried_while_cond_closure_loop_and_value_updated_return_reject_e3104() {
    for body in [
        "m := map[string]entry{\"k\": entry{val: 1, arr: [2]int{0, 0}}}\nvar slot *entry\nfor slot != nil && check_int_ptr(&slot.val) {\n    slot = &m[\"k\"]\n    slot.val = 0\n}",
        "m := map[string]entry{\"k\": entry{val: 1, arr: [2]int{0, 0}}}\nvar slot *entry\nfor slot != nil && check_ptr(move slot) {\n    slot = &m[\"k\"]\n}",
        "f := fn() {\n    m := map[string]entry{\"k\": entry{val: 1, arr: [2]int{0, 0}}}\n    var slot *entry\n    for i in 0..2 {\n        if i == 1 {\n            task consume_ptr(move slot)\n        }\n        slot = &m[\"k\"]\n        slot.val = i\n    }\n}\nf()",
        "f := fn() {\n    m := map[string]entry{\"k\": entry{val: 1, arr: [2]int{0, 0}}}\n    var slot *entry\n    other := entry{val: 9, arr: [2]int{0, 0}}\n    for i in 0..2 {\n        if i == 1 {\n            slot = &other\n        }\n        slot = &m[\"k\"]\n        slot.val = i\n    }\n}\nf()",
        "f := fn() *entry {\n    m := map[string]entry{\"k\": entry{val: 1, arr: [2]int{0, 0}}}\n    var q entry\n    q = &m[\"k\"]\n    q = entry{val: 2, arr: [2]int{0, 0}}\n    return q\n}\n_ = f()",
    ] {
        assert_e3104(&format!("{PRE}\nfn main() {{\n{body}\n}}\n"));
    }
    assert_e3104(&format!("{PRE}\nfn bad_alias_return() *entry {{\n    m := map[string]entry{{\"k\": entry{{val: 1, arr: [2]int{{0, 0}}}}}}\n    var q entry\n    q = &m[\"k\"]\n    q = entry{{val: 2, arr: [2]int{{0, 0}}}}\n    p := q\n    return p\n}}\nfn main() {{ _ = bad_alias_return() }}\n"));
}

#[test]
fn non_loan_parallel_rebind_and_shadowed_loop_bindings_stay_accepted() {
    for body in [
        "m := map[string]entry{\"k\": entry{val: 1, arr: [2]int{0, 0}}}\nxs := []entry{entry{val: 5, arr: [2]int{0, 0}}}\nq := &m[\"k\"]\ntag := 0\ntag, q = 1, &xs[0]\nq.val = tag + 2",
        "m := map[string]entry{\"k\": entry{val: 1, arr: [2]int{0, 0}}}\nslot := entry{val: 3, arr: [2]int{0, 0}}\nfor check_int_ptr(&slot.val) {\n    var slot *entry\n    slot = &m[\"k\"]\n    slot.val = 0\n    break\n}",
        "m := map[string]entry{\"k\": entry{val: 1, arr: [2]int{0, 0}}}\nslot := entry{val: 3, arr: [2]int{0, 0}}\nother := entry{val: 4, arr: [2]int{0, 0}}\nfor check_int_ptr(&slot.val) {\n    if slot := &other; slot != nil {\n        slot = &m[\"k\"]\n        slot.val = 0\n    }\n    break\n}",
        "f := fn() {\n    m := map[string]entry{\"k\": entry{val: 1, arr: [2]int{0, 0}}}\n    for i in 0..2 {\n        slot := &m[\"k\"]\n        slot.val = i\n        if true {\n            slot := entry{val: 7, arr: [2]int{0, 0}}\n            p := &slot\n            _ = p.val\n        }\n    }\n}\nf()",
    ] {
        assert_accept(&format!("{PRE}\nfn main() {{\n{body}\n}}\n"));
    }
}
