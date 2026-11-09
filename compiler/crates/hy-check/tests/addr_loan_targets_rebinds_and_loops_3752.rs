use hy_check::{Verdict, check_file};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let parsed = parse_file("test.hy", src);
    assert!(
        parsed.diagnostics.is_empty(),
        "parse failed: {:?}",
        parsed.diagnostics
    );
    check_file(&parsed.file)
}

#[test]
fn rejects_place_addr_of_targets_on_slot_loan_and_map_3752() {
    let r = check(
        "package main
         struct entry { val int }
         fn main() {
             m := map[string]entry{"k": entry{val: 1}}
             q := &m["k"]
             other := entry{val: 2}
             x := 3
             &q = &other
             &q.val = &x
             &m["k"] = &other
             &m["k"].val = &x
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    let e3104 = r.diagnostics.iter().filter(|d| d.code == "HYB-E3104").count();
    assert!(e3104 >= 4, "{:?}", r.diagnostics);
}

#[test]
fn rejects_mixed_target_parallel_rebind_and_move_wrapped_addr_of_3752() {
    let r = check(
        "package main
         struct entry { val int }
         fn main() {
             m := map[string]entry{"k": entry{val: 1}}
             xs := []entry{entry{val: 2}}
             sink := []int{0}
             q := &m["k"]
             sink[0], q = 1, &xs[0]
             p1 := &(move m["k"])
             p2 := &(move m["k"].val)
             _ = q
             _ = p1
             _ = p2
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    let e3104 = r.diagnostics.iter().filter(|d| d.code == "HYB-E3104").count();
    assert!(e3104 >= 3, "{:?}", r.diagnostics);
}

#[test]
fn rejects_loop_carried_slot_loan_in_while_cond_closure_loop_and_value_updated_return_3752() {
    let r = check(
        "package main
         struct entry { val int }
         fn check_int_ptr(p *int) bool { return p != nil }
         fn leak() *entry {
             m := map[string]entry{"k": entry{val: 1}}
             q := &m["k"]
             q = entry{val: 2}
             p := q
             return p
         }
         fn main() {
             m := map[string]entry{"k": entry{val: 1}}
             var slot *entry
             for slot == nil || check_int_ptr(&slot.val) {
                 slot = &m["k"]
                 break
             }
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    let e3104 = r.diagnostics.iter().filter(|d| d.code == "HYB-E3104").count();
    assert!(e3104 >= 2, "{:?}", r.diagnostics);
}
