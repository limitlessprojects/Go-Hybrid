//! mixed_assign_slot_loan_accept_3608.rs: positive control tests for #3608.
//!
//! Slot-loan aliases bound via mixed-target parallel assignment remain valid
//! for direct reads, field writes, and shadowed non-loan bindings.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn assert_accept(src: &str) {
    let r = check_file(&parse_file("inline.hy", src).expect("snippet parses"));
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected Accept, got {:?} for:\n{src}",
        r.diagnostics
    );
    assert!(
        r.diagnostics.is_empty(),
        "expected 0 diagnostics, got {:?} for:\n{src}",
        r.diagnostics
    );
}

const PRE: &str = r#"
struct entry {
    val int
    arr [2]int
}

struct tag_box {
    tag int
}
"#;

#[test]
fn mixed_assign_slot_loan_direct_access_shadowing_and_plain_ptr_accept() {
    let test_cases = [
        // 1. Direct field read and write via q2 alias
        r#"
m := map[string]entry{"k": entry{val: 1, arr: [2]int{3, 4}}}
q1 := &m["k"]
var q2 *entry
sink := []int{0}
sink[0], q2 = 1, q1
q2.val = 99 + sink[0] + q2.arr[0]
_ = m["k"].val
"#,
        // 2. Struct-field non-ident place with parenthesized alias (q1)
        r#"
m := map[string]entry{"k": entry{val: 1, arr: [2]int{0, 0}}}
q1 := &m["k"]
var q2 *entry
bx := tag_box{tag: 0}
bx.tag, q2 = 7, (q1)
q2.val = bx.tag
_ = m["k"].val
"#,
        // 3. Shadowed q2 inside inner block is a local value, not a slot loan
        r#"
m := map[string]entry{"k": entry{val: 1, arr: [2]int{0, 0}}}
q1 := &m["k"]
var q2 *entry
sink := []int{0}
sink[0], q2 = 1, q1
q2.val = 5
if true {
    q2 := entry{val: 8, arr: [2]int{1, 2}}
    p := &q2
    _ = p.val
}
"#,
        // 4. Plain non-slot pointer in mixed assignment allows address-of field
        r#"
loc := entry{val: 1, arr: [2]int{0, 0}}
raw := &loc
var q2 *entry
sink := []int{0}
sink[0], q2 = 1, raw
p := &q2.val
_ = p
other := entry{val: 2, arr: [2]int{0, 0}}
q2 = &other
_ = q2.val
"#,
    ];

    for body in test_cases {
        assert_accept(&format!("{PRE}\nfn main() {{\n{body}\n}}\n"));
    }
}
