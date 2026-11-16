//! T24 (`HYB-E3104`) — slot-loan aliases bound via mixed-target parallel assignment remain valid for direct reads, field writes, and shadowed non-loan bindings.
use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn assert_accept(src: &str) {
    let r = check_file(&parse_file("t.hy", src).expect("snippet parses"));
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept, got {:?} for:\n{src}", r.diagnostics);
    assert!(r.diagnostics.is_empty(), "expected 0 diagnostics, got {:?} for:\n{src}", r.diagnostics);
}

const PRE: &str = "struct entry {\n    val int\n    arr [2]int\n}\nstruct tag_box { tag int }";

#[test]
fn mixed_assign_slot_loan_direct_access_shadowing_and_plain_ptr_accept() {
    for body in [
        "m := map[string]entry{\"k\": entry{val: 1, arr: [2]int{3, 4}}}\nq1 := &m[\"k\"]\nvar q2 *entry\nsink := []int{0}\nsink[0], q2 = 1, q1\nq2.val = 99 + sink[0] + q2.arr[0]\n_ = m[\"k\"].val",
        "m := map[string]entry{\"k\": entry{val: 1, arr: [2]int{0, 0}}}\nq1 := &m[\"k\"]\nvar q2 *entry\nbx := tag_box{tag: 0}\nbx.tag, q2 = 7, (q1)\nq2.val = bx.tag\n_ = m[\"k\"].val",
        "m := map[string]entry{\"k\": entry{val: 1, arr: [2]int{0, 0}}}\nq1 := &m[\"k\"]\nvar q2 *entry\nsink := []int{0}\nsink[0], q2 = 1, q1\nq2.val = 5\nif true {\n    q2 := entry{val: 8, arr: [2]int{1, 2}}\n    p := &q2\n    _ = p.val\n}",
        "loc := entry{val: 1, arr: [2]int{0, 0}}\nraw := &loc\nvar q2 *entry\nsink := []int{0}\nsink[0], q2 = 1, raw\np := &q2.val\n_ = p\nother := entry{val: 2, arr: [2]int{0, 0}}\nq2 = &other\n_ = q2.val",
    ] {
        assert_accept(&format!("{PRE}\nfn main() {{\n{body}\n}}\n"));
    }
}
