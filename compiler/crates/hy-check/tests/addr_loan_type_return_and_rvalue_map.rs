use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let parsed = parse_file("test.hy", src).expect("parse ok");
    check_file(&parsed)
}

#[test]
fn rejects_t24_in_type_expr_array_lengths_and_nested_return_map_borrow() {
    for src in [
        "struct acct { bal int }\nfn f(p *int) int { _ = p\n return 1 }\nfn g(m map[string]acct) { n := &m[\"a\"]\n var a [f(&n.bal)]int\n _ = a }",
        "struct acct { bal int }\nfn f(p *acct) int { return p.bal }\nfn g(m map[string]acct) { _ = [f(&m[\"a\"])]int{1} }",
        "struct acct { bal int }\nfn f(p *int) int { _ = p\n return 1 }\nfn g(m map[string]acct) { n := &m[\"a\"]\n _ = map[[f(&n.bal)]int]int{} }",
        "struct acct { bal int }\nfn f(p *int) int { _ = p\n return 1 }\nfn g(m map[string]acct) { n := &m[\"a\"]\n h := fn(x [f(&n.bal)]int) { _ = x }\n _ = h }",
    ] {
        let r = check(src);
        assert!(r.verdict == Verdict::Reject && r.diagnostics.iter().any(|d| d.code == "HYB-E3104"), "{:?}", r.diagnostics);
    }
    let r = check("struct acct { bal int }\nfn k(p *int) string { _ = p\n return \"a\" }\nfn g(m map[string]acct) *acct { n := &m[\"a\"]\n return &m[k(&n.bal)] }");
    assert_eq!(r.diagnostics.iter().filter(|d| d.code == "HYB-E3104").count(), 2, "{:?}", r.diagnostics);
}

#[test]
fn rejects_non_place_map_slot_borrow_in_local_and_assign() {
    for src in [
        "struct acct { bal int }\nfn get_map() map[string]acct { return map[string]acct{\"a\": acct{bal: 1}} }\nfn g() { n := &get_map()[\"a\"]\n _ = n }",
        "struct acct { bal int }\nfn g() { n := &(map[string]acct{\"a\": acct{bal: 1}}[\"a\"])\n _ = n }",
        "struct acct { bal int }\nfn get_map() map[string]acct { return map[string]acct{\"a\": acct{bal: 1}} }\nfn g(m map[string]acct) { n := &m[\"a\"]\n n = &get_map()[\"a\"]\n _ = n }",
    ] {
        let r = check(src);
        assert!(r.verdict == Verdict::Reject && r.diagnostics.iter().any(|d| d.code == "HYB-E3104"), "{:?}", r.diagnostics);
    }
}
