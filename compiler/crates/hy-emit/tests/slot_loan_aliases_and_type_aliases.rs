//! Issue #2703: O8 slot-loan lowering for pointer type aliases (`type P = *T`),
//! local alias bindings (`p := s`, `var p *T = s`, `p = s`), and same-statement
//! parallel key mutations (`s, k := &m[k], "b"` / `s, k = &m[k], "b"`).

fn emit(src: &str) -> String {
    let file = hy_syntax::parse_file("t.hy", src).expect("parse");
    hy_emit::emit_go(&file).unwrap_or_else(|e| panic!("emit failed: {}", e.message))
}

#[test]
fn pointer_type_alias_var_and_named_result_lower_to_pointer_slot_loan() {
    let go = emit(
        "struct acct {\n    bal int\n}\n\ntype acct_ptr = *acct\ntype acct_ptr2 = acct_ptr\n\nfn update(m map[string]acct) (out acct_ptr2) {\n    var p acct_ptr\n    p = &m[\"a\"]\n    p.bal = 10\n    out = &m[\"b\"]\n    out.bal = 20\n    return\n}\n",
    );
    assert!(go.contains("hyLoan0 := m[\"a\"]"), "{go}");
    assert!(go.contains("p = &hyLoan0"), "{go}");
    assert!(go.contains("m[\"a\"] = *p"), "{go}");
    assert!(go.contains("hyLoan1 := m[\"b\"]"), "{go}");
    assert!(go.contains("out = &hyLoan1"), "{go}");
    assert!(go.contains("m[\"b\"] = *out"), "{go}");
}

#[test]
fn pointer_type_alias_param_and_ptr_returning_fn_keep_pointer_slot_loan() {
    let go = emit(
        "struct acct {\n    bal int\n}\n\ntype acct_ptr = *acct\n\nfn seed(a *acct) acct_ptr {\n    return a\n}\n\nfn bump(m map[string]acct, p acct_ptr) {\n    p = &m[\"a\"]\n    p.bal += 1\n    q := seed(p)\n    q = &m[\"b\"]\n    q.bal += 2\n}\n",
    );
    assert!(go.contains("hyLoan0 := m[\"a\"]"), "{go}");
    assert!(go.contains("p = &hyLoan0"), "{go}");
    assert!(go.contains("m[\"a\"] = *p"), "{go}");
    assert!(go.contains("hyLoan1 := m[\"b\"]"), "{go}");
    assert!(go.contains("q = &hyLoan1"), "{go}");
    assert!(go.contains("m[\"b\"] = *q"), "{go}");
}

#[test]
fn value_slot_loan_aliased_by_define_var_and_assign_addresses_copy_and_delays_flush() {
    let go = emit(
        "struct acct {\n    bal int\n}\n\nfn worker(m map[string]acct, done chan bool) {\n    done <- true\n}\n\nfn main() {\n    m := map[string]acct{\"a\": acct{bal: 1}}\n    done := make(chan bool, 1)\n    s := &m[\"a\"]\n    p := s\n    var q *acct = s\n    var r *acct\n    r = s\n    p.bal = 10\n    q.bal = 20\n    r.bal = 30\n    task worker(move m, done)\n    <-done\n}\n",
    );
    assert!(go.contains("s := m[\"a\"]"), "{go}");
    assert!(go.contains("p := &s"), "{go}");
    assert!(go.contains("var q *acct = &s"), "{go}");
    assert!(go.contains("r = &s"), "{go}");
    let last_mut = go.find("r.bal = 30").expect("last alias mutation");
    let flush_at = go.find("m[\"a\"] = s").expect("slot-loan store-back");
    let spawn_at = go.find("go worker(m, done)").expect("task spawn");
    assert!(
        last_mut < flush_at && flush_at < spawn_at,
        "store-back must flush after last alias mutation and before task move:\n{go}"
    );
}

#[test]
fn pointer_slot_loan_alias_keeps_store_back_alive_across_branch_handoff() {
    let go = emit(
        "struct acct {\n    bal int\n}\n\nfn worker(m map[string]acct, done chan bool) {\n    done <- true\n}\n\nfn run(cond bool) {\n    m := map[string]acct{\"a\": acct{bal: 1}}\n    done := make(chan bool, 1)\n    var s *acct\n    s = &m[\"a\"]\n    p := s\n    if cond {\n        p.bal = 99\n        task worker(move m, done)\n        <-done\n    }\n}\n",
    );
    let mut_in_then = go.find("p.bal = 99").expect("alias mutation in then");
    let flush_in_then = go.find("m[\"a\"] = *s").expect("store-back in then");
    let spawn_in_then = go.find("go worker(m, done)").expect("spawn in then");
    assert!(
        mut_in_then < flush_in_then && flush_in_then < spawn_in_then,
        "pointer slot-loan store-back must happen after alias mutation and before move:\n{go}"
    );
    assert_eq!(
        go.matches("m[\"a\"] = *s").count(),
        2,
        "both branches must flush the handed-off slot loan:\n{go}"
    );
}

#[test]
fn parallel_define_and_assign_overwriting_key_snapshots_hykey() {
    let go = emit(
        "struct acct {\n    bal int\n}\n\nfn main() {\n    m := map[string]acct{\"a\": acct{bal: 1}, \"b\": acct{bal: 2}}\n    k := \"a\"\n    s, k := &m[k], \"b\"\n    s.bal = 10\n    var p *acct\n    p, k = &m[k], \"a\"\n    p.bal = 20\n    fmt.println(k)\n}\n",
    );
    assert!(go.contains("hyKey0 := k"), "{go}");
    assert!(go.contains("s, k := m[hyKey0], \"b\""), "{go}");
    assert!(go.contains("m[hyKey0] = s"), "{go}");
    assert!(go.contains("hyKey1 := k"), "{go}");
    assert!(go.contains("hyLoan2 := m[hyKey1]"), "{go}");
    assert!(go.contains("m[hyKey1] = *p"), "{go}");
}
