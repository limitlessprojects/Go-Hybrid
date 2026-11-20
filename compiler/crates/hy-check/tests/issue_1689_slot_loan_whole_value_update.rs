use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

#[test]
fn whole_value_update_on_active_slot_loan_links_map_and_rhs_e3003() {
    let r = check(
        "package main\n\
         struct acct { tags map[string]int }\n\
         fn worker(m map[string]acct) { m[\"a\"].tags[\"x\"] = 2 }\n\
         fn main() {\n\
           m := map[string]acct{\"a\": acct{tags: map[string]int{\"x\": 1}}}\n\
           fresh := acct{tags: map[string]int{\"x\": 10}}\n\
           var q acct\n\
           q = &m[\"a\"]\n\
           q = fresh\n\
           _ = q.tags[\"x\"]\n\
           task worker(move m)\n\
           fresh.tags[\"x\"] = 99\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 when writing to fresh after moving loaned map m, got {:?}",
        r.diagnostics
    );
}

#[test]
fn whole_value_update_on_active_slot_loan_moving_rhs_freezes_map_e3003() {
    let r = check(
        "package main\n\
         struct acct { tags map[string]int }\n\
         fn worker(a acct) { a.tags[\"x\"] = 2 }\n\
         fn main() {\n\
           m := map[string]acct{\"a\": acct{tags: map[string]int{\"x\": 1}}}\n\
           fresh := acct{tags: map[string]int{\"x\": 10}}\n\
           var q acct\n\
           q = &m[\"a\"]\n\
           q = fresh\n\
           _ = q.tags[\"x\"]\n\
           task worker(move fresh)\n\
           m[\"a\"].tags[\"x\"] = 99\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 when writing through map m after moving fresh, got {:?}",
        r.diagnostics
    );
}

#[test]
fn whole_value_update_on_active_slot_loan_propagates_non_owned_param_origin_e3004() {
    let r = check(
        "package main\n\
         struct acct { tags map[string]int }\n\
         fn sink(m map[string]acct) { m[\"a\"].tags[\"x\"] = 2 }\n\
         fn launder(p acct) {\n\
           m := map[string]acct{\"a\": acct{tags: map[string]int{\"x\": 1}}}\n\
           var q acct\n\
           q = &m[\"a\"]\n\
           q = p\n\
           _ = q.tags[\"x\"]\n\
           task sink(move m)\n\
         }\n\
         fn main() {\n\
           caller := acct{tags: map[string]int{\"x\": 10}}\n\
           launder(caller)\n\
           caller.tags[\"x\"] = 99\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
        "expected HYB-E3004 when moving map m after storing non-owned parameter p via slot loan, got {:?}",
        r.diagnostics
    );
}

#[test]
fn whole_value_update_on_active_slot_loan_propagates_const_origin_e3006() {
    let r = check(
        "package main\n\
         struct acct { tags map[string]int }\n\
         const shared_acct = acct{tags: map[string]int{\"x\": 1}}\n\
         fn main() {\n\
           m := map[string]acct{\"a\": acct{tags: map[string]int{\"y\": 2}}}\n\
           var q acct\n\
           q = &m[\"a\"]\n\
           q = shared_acct\n\
           _ = q.tags[\"x\"]\n\
           m[\"a\"].tags[\"x\"] = 99\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "expected HYB-E3006 when mutating map m after storing const shared_acct via slot loan, got {:?}",
        r.diagnostics
    );
}

#[test]
fn multi_return_update_on_define_slot_loan_links_map_and_rhs_e3003() {
    let r = check(
        "package main\n\
         struct acct { tags map[string]int }\n\
         fn pair_acct(a acct) (acct, bool) { return a, true }\n\
         fn worker(m map[string]acct) { m[\"a\"].tags[\"x\"] = 2 }\n\
         fn main() {\n\
           m := map[string]acct{\"a\": acct{tags: map[string]int{\"x\": 1}}}\n\
           fresh := acct{tags: map[string]int{\"x\": 10}}\n\
           q := &m[\"a\"]\n\
           var ok bool\n\
           q, ok = pair_acct(fresh)\n\
           _ = ok\n\
           _ = q.tags[\"x\"]\n\
           task worker(move m)\n\
           fresh.tags[\"x\"] = 99\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 for multi-return update on active slot loan, got {:?}",
        r.diagnostics
    );
}

#[test]
fn callee_param_store_via_slot_loan_whole_value_update_e3003() {
    let r = check(
        "package main\n\
         struct acct { tags map[string]int }\n\
         fn install(m map[string]acct, v acct) {\n\
           var q acct\n\
           q = &m[\"a\"]\n\
           q = v\n\
           _ = q.tags[\"x\"]\n\
         }\n\
         fn worker(m map[string]acct) { m[\"a\"].tags[\"x\"] = 2 }\n\
         fn main() {\n\
           m := map[string]acct{\"a\": acct{tags: map[string]int{\"x\": 1}}}\n\
           fresh := acct{tags: map[string]int{\"x\": 10}}\n\
           install(m, fresh)\n\
           task worker(move m)\n\
           fresh.tags[\"x\"] = 99\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 after helper stores parameter into map via slot loan update, got {:?}",
        r.diagnostics
    );
}

#[test]
fn closure_const_write_via_slot_loan_whole_value_update_e3006() {
    let r = check(
        "package main\n\
         struct acct { tags map[string]int }\n\
         const shared_acct = acct{tags: map[string]int{\"x\": 1}}\n\
         fn main() {\n\
           f := fn() {\n\
             m := map[string]acct{\"a\": acct{tags: map[string]int{\"y\": 2}}}\n\
             var q acct\n\
             q = &m[\"a\"]\n\
             q = shared_acct\n\
             _ = q.tags[\"x\"]\n\
             m[\"a\"].tags[\"x\"] = 99\n\
           }\n\
           f()\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "expected HYB-E3006 inside closure body after storing const into slot loan, got {:?}",
        r.diagnostics
    );
}

#[test]
fn whole_value_update_with_fresh_literal_and_no_post_move_use_accepts() {
    let r = check(
        "package main\n\
         struct acct { tags map[string]int }\n\
         fn worker(m map[string]acct) { m[\"a\"].tags[\"x\"] = 2 }\n\
         fn main() {\n\
           m := map[string]acct{\"a\": acct{tags: map[string]int{\"x\": 1}}}\n\
           var q acct\n\
           q = &m[\"a\"]\n\
           q = acct{tags: map[string]int{\"x\": 10}}\n\
           _ = q.tags[\"x\"]\n\
           task worker(move m)\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
}
