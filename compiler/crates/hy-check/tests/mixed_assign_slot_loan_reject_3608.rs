//! mixed_assign_slot_loan_reject_3608.rs: regression tests for #3608.
//!
//! Under T24 (SPEC-LANGUAGE.md §2.9, `HYB-E3104`), slot-loan aliases propagated through
//! mixed-target parallel assignment (`sink[0], q2 = 1, q1`) must maintain `Binding::slot_loan = true`
//! and be rejected on address-of, move, rebind, or chained borrows.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn assert_e3104(src: &str) {
    let r = check_file(&parse_file("inline.hy", src).expect("snippet parses"));
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{src}");
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3104"),
        "expected HYB-E3104, got {:?} for:\n{src}",
        r.diagnostics
    );
}

const PRE: &str = r#"
import "fmt"

struct entry {
    val int
    arr [2]int
}

struct tag_box {
    tag int
}

fn next_entry() (*entry, bool) {
    e := entry{val: 5, arr: [2]int{0, 0}}
    return &e, true
}

fn consume_ptr(p *entry) {
    _ = p.val
}
"#;

#[test]
fn mixed_assign_slot_loan_alias_rejects_addr_move_rebind_and_direct_borrow() {
    let test_cases = [
        // 1. Reproducer from #3608: address-of field on alias bound via mixed parallel assignment
        r#"
m := map[string]entry{"k": entry{val: 1, arr: [2]int{0, 0}}}
q1 := &m["k"]
var q2 *entry
sink := []int{0}
sink[0], q2 = 1, q1
p := &q2.val
_, _ = fmt.sscan("99", p)
"#,
        // 2. Address-of array element through parenthesized alias
        r#"
m := map[string]entry{"k": entry{val: 1, arr: [2]int{2, 3}}}
q1 := &m["k"]
var q2 *entry
sink := []int{0}
sink[0], q2 = 1, (q1)
p := &q2.arr[0]
_ = p
"#,
        // 3. Address-of pointer variable itself with struct-field non-ident place
        r#"
m := map[string]entry{"k": entry{val: 1, arr: [2]int{0, 0}}}
q1 := &m["k"]
var q2 *entry
bx := tag_box{tag: 0}
bx.tag, q2 = 1, q1
p := &q2
_ = p
"#,
        // 4. Move of slot-loan alias inside task spawn
        r#"
m := map[string]entry{"k": entry{val: 1, arr: [2]int{0, 0}}}
q1 := &m["k"]
var q2 *entry
bx := tag_box{tag: 0}
bx.tag, q2 = 1, q1
task consume_ptr(move q2)
"#,
        // 5. Rebinding slot-loan alias to general pointer
        r#"
m := map[string]entry{"k": entry{val: 1, arr: [2]int{0, 0}}}
q1 := &m["k"]
var q2 *entry
bx := tag_box{tag: 0}
bx.tag, q2 = 1, q1
other := entry{val: 9, arr: [2]int{0, 0}}
q2 = &other
_ = q2.val
"#,
        // 6. Multi-return rebind of slot-loan alias
        r#"
m := map[string]entry{"k": entry{val: 1, arr: [2]int{0, 0}}}
q1 := &m["k"]
var q2 *entry
var ok bool
sink := []int{0}
sink[0], q2 = 1, q1
q2, ok = next_entry()
_ = ok
"#,
        // 7. Chained mixed-target propagation: q1 -> q2 -> q3
        r#"
m := map[string]entry{"k": entry{val: 1, arr: [2]int{0, 0}}}
q1 := &m["k"]
var q2 *entry
var q3 *entry
sink := []int{0}
bx := tag_box{tag: 0}
sink[0], q2 = 1, q1
bx.tag, q3 = 2, q2
p := &q3.val
_ = p
"#,
        // 8. Closure-local mixed assignment propagation
        r#"
f := fn() {
    m := map[string]entry{"k": entry{val: 1, arr: [2]int{0, 0}}}
    q1 := &m["k"]
    var q2 *entry
    sink := []int{0}
    sink[0], q2 = 1, q1
    p := &q2
    _ = p
}
f()
"#,
        // 9. Direct &m[k] in mixed assignment (rejected)
        r#"
m := map[string]entry{"k": entry{val: 1, arr: [2]int{0, 0}}}
var q2 *entry
sink := []int{0}
sink[0], q2 = 1, &m["k"]
_ = q2
"#,
    ];

    for body in test_cases {
        assert_e3104(&format!("{PRE}\nfn main() {{\n{body}\n}}\n"));
    }
}
