//! T24 (`HYB-E3104`) — mixed-target parallel assignment propagates `slot_loan` to identifier targets while still rejecting direct map-slot borrows.
use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn assert_e3104(src: &str) {
    let r = check_file(&parse_file("t.hy", src).expect("snippet parses"));
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{src}");
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3104"),
        "expected HYB-E3104, got {:?} for:\n{src}",
        r.diagnostics
    );
}

const PRE: &str = "import \"fmt\"\nstruct entry {\n    val int\n    arr [2]int\n}\nstruct tag_box { tag int }\nfn next_entry() (*entry, bool) {\n    e := entry{val: 5, arr: [2]int{0, 0}}\n    return &e, true\n}\nfn consume_ptr(p *entry) { _ = p.val }";

#[test]
fn mixed_assign_slot_loan_alias_rejects_addr_move_rebind_and_direct_borrow() {
    for body in [
        "m := map[string]entry{\"k\": entry{val: 1, arr: [2]int{0, 0}}}\nq1 := &m[\"k\"]\nvar q2 *entry\nsink := []int{0}\nsink[0], q2 = 1, q1\np := &q2.val\n_, _ = fmt.sscan(\"99\", p)",
        "m := map[string]entry{\"k\": entry{val: 1, arr: [2]int{2, 3}}}\nq1 := &m[\"k\"]\nvar q2 *entry\nsink := []int{0}\nsink[0], q2 = 1, (q1)\np := &q2.arr[0]\n_ = p",
        "m := map[string]entry{\"k\": entry{val: 1, arr: [2]int{0, 0}}}\nq1 := &m[\"k\"]\nvar q2 *entry\nbx := tag_box{tag: 0}\nbx.tag, q2 = 1, q1\np := &q2\n_ = p",
        "m := map[string]entry{\"k\": entry{val: 1, arr: [2]int{0, 0}}}\nq1 := &m[\"k\"]\nvar q2 *entry\nbx := tag_box{tag: 0}\nbx.tag, q2 = 1, q1\ntask consume_ptr(move q2)",
        "m := map[string]entry{\"k\": entry{val: 1, arr: [2]int{0, 0}}}\nq1 := &m[\"k\"]\nvar q2 *entry\nbx := tag_box{tag: 0}\nbx.tag, q2 = 1, q1\nother := entry{val: 9, arr: [2]int{0, 0}}\nq2 = &other\n_ = q2.val",
        "m := map[string]entry{\"k\": entry{val: 1, arr: [2]int{0, 0}}}\nq1 := &m[\"k\"]\nvar q2 *entry\nvar ok bool\nsink := []int{0}\nsink[0], q2 = 1, q1\nq2, ok = next_entry()\n_ = ok",
        "m := map[string]entry{\"k\": entry{val: 1, arr: [2]int{0, 0}}}\nq1 := &m[\"k\"]\nvar q2 *entry\nvar q3 *entry\nsink := []int{0}\nbx := tag_box{tag: 0}\nsink[0], q2 = 1, q1\nbx.tag, q3 = 2, q2\np := &q3.val\n_ = p",
        "f := fn() {\n    m := map[string]entry{\"k\": entry{val: 1, arr: [2]int{0, 0}}}\n    q1 := &m[\"k\"]\n    var q2 *entry\n    sink := []int{0}\n    sink[0], q2 = 1, q1\n    p := &q2\n    _ = p\n}\nf()",
        "m := map[string]entry{\"k\": entry{val: 1, arr: [2]int{0, 0}}}\nvar q2 *entry\nsink := []int{0}\nsink[0], q2 = 1, &m[\"k\"]\n_ = q2",
    ] {
        assert_e3104(&format!("{PRE}\nfn main() {{\n{body}\n}}\n"));
    }
}
