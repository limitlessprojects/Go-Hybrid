use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check_codes(src: &str) -> (Verdict, Vec<String>) {
    let parsed = parse_file("test.hy", src).expect("test snippet must parse");
    let res = check_file(&parsed);
    let codes = res.diagnostics.iter().map(|d| d.code.clone()).collect();
    (res.verdict, codes)
}

#[test]
fn field_projected_map_slot_loan_rejects_live_insert_and_delete() {
    let insert_src = "import \"fmt\"\nstruct item { f int }\nfn bad_insert(m map[string]*item, next *item) {\n    p := &m[\"a\"].f\n    m[\"b\"] = next\n    fmt.println(p)\n}\n";
    let (verdict, codes) = check_codes(insert_src);
    assert_eq!(verdict, Verdict::Reject, "expected Reject, got: {codes:?}");
    assert!(codes.iter().any(|c| c == "HYB-E3102"), "got: {codes:?}");

    let delete_src = "import \"fmt\"\nstruct inner { v int }\nstruct outer { nested inner }\nfn bad_delete(m map[string]*outer) {\n    p := &((m[\"a\"]).nested.v)\n    delete(m, \"a\")\n    fmt.println(p)\n}\n";
    let (verdict, codes) = check_codes(delete_src);
    assert_eq!(verdict, Verdict::Reject, "expected Reject, got: {codes:?}");
    assert!(codes.iter().any(|c| c == "HYB-E3102"), "got: {codes:?}");
}

#[test]
fn move_wrapped_and_reborrowed_field_projected_slot_loan_rejected() {
    let move_src = "import \"fmt\"\nstruct item { f int }\nfn bad_move_slot_loan(m map[string]*item, next *item) {\n    p := move(&m[\"a\"].f)\n    m[\"b\"] = next\n    fmt.println(p)\n}\n";
    let (verdict, codes) = check_codes(move_src);
    assert_eq!(verdict, Verdict::Reject, "expected Reject, got: {codes:?}");
    assert!(codes.iter().any(|c| c == "HYB-E3102"), "got: {codes:?}");

    let addr_of_loan_src = "import \"fmt\"\nstruct item { f int }\nfn bad_reborrow(m map[string]*item) {\n    p := &m[\"a\"].f\n    q := &p\n    fmt.println(q)\n}\n";
    let (verdict, codes) = check_codes(addr_of_loan_src);
    assert_eq!(verdict, Verdict::Reject, "expected Reject, got: {codes:?}");
    assert!(codes.iter().any(|c| c == "HYB-E3104"), "got: {codes:?}");
}

#[test]
fn field_projected_map_slot_loan_positive_control_accepts_after_last_use() {
    let ok_src = "import \"fmt\"\nstruct item { f int }\nfn valid_use_before_mutation(m map[string]*item, next *item) {\n    p := &m[\"a\"].f\n    fmt.println(p)\n    m[\"b\"] = next\n}\n";
    let (verdict, codes) = check_codes(ok_src);
    assert_eq!(verdict, Verdict::Accept, "expected Accept, got: {codes:?}");
}

#[test]
fn bare_ident_match_pattern_is_a_reference_so_use_after_move_rejected() {
    let src = "import \"fmt\"\nfn sink(m map[string]int) { fmt.println(len(m)) }\nfn f(sel map[string]int) {\n    owned := map[string]int{\"k\": 1}\n    sink(move owned)\n    match sel {\n        owned: { fmt.println(len(owned)) }\n        _: { fmt.println(0) }\n    }\n}\n";
    let (verdict, codes) = check_codes(src);
    assert_eq!(verdict, Verdict::Reject, "expected Reject, got: {codes:?}");
    assert!(codes.iter().any(|c| c == "HYB-E3003"), "got: {codes:?}");
}
