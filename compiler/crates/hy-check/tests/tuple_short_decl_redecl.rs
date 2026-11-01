//! Integration tests for same-scope multi-variable short redeclaration (`:=`)
//! of active O8 map-slot loans (`addr_loan.rs` / `emit_guard.rs`):
//! Rebinding a live slot loan in the same lexical scope via parallel or
//! multi-return `:=` to a general pointer (`q, tag := &acct{bal: 5}, 1`,
//! `q, tag := nil, 1`, or `q, ok := next_acct()`) must reject with `HYB-E3104`
//! (T24 + O8), while inner-block shadowing remains accepted.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check_codes(src: &str) -> (Verdict, Vec<String>) {
    let ast = parse_file("test.hy", src).expect("source must parse");
    let out = check_file(&ast);
    let codes = out
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect();
    (out.verdict, codes)
}

#[test]
fn same_scope_parallel_short_redecl_of_slot_loan_to_pointer_rejects_e3104() {
    let src = r#"
struct acct {
    bal int
}

fn main() {
    m := map[string]acct{"a": acct{bal: 1}}
    q := &m["a"]
    q, tag := &acct{bal: 5}, 1
    _ = q.bal
    _ = tag
}
"#;
    let (verdict, codes) = check_codes(src);
    assert_eq!(verdict, Verdict::Reject, "expected Reject, got {:?}", codes);
    assert!(
        codes.contains(&"HYB-E3104".to_string()),
        "expected HYB-E3104, got {:?}",
        codes
    );
}

#[test]
fn same_scope_multi_return_short_redecl_of_slot_loan_to_pointer_rejects_e3104() {
    let src = r#"
struct acct {
    bal int
}

fn next_acct(p *acct) (*acct, bool) {
    return p, true
}

fn main() {
    a := acct{bal: 5}
    m := map[string]acct{"a": acct{bal: 1}}
    q := &m["a"]
    q, ok := next_acct(&a)
    _ = q.bal
    _ = ok
}
"#;
    let (verdict, codes) = check_codes(src);
    assert_eq!(verdict, Verdict::Reject, "expected Reject, got {:?}", codes);
    assert!(
        codes.contains(&"HYB-E3104".to_string()),
        "expected HYB-E3104, got {:?}",
        codes
    );
}

#[test]
fn closure_body_same_scope_short_redecl_of_slot_loan_to_pointer_rejects_e3104() {
    let src = r#"
struct acct {
    bal int
}

fn main() {
    f := fn() {
        m := map[string]acct{"a": acct{bal: 1}}
        q := &m["a"]
        q, tag := &acct{bal: 5}, 1
        _ = q.bal
        _ = tag
    }
    f()
}
"#;
    let (verdict, codes) = check_codes(src);
    assert_eq!(verdict, Verdict::Reject, "expected Reject, got {:?}", codes);
    assert!(
        codes.contains(&"HYB-E3104".to_string()),
        "expected HYB-E3104, got {:?}",
        codes
    );
}

#[test]
fn inner_block_shadowing_short_decl_of_slot_loan_to_pointer_accepts() {
    let src = r#"
struct acct {
    bal int
}

fn main() {
    m := map[string]acct{"a": acct{bal: 1}}
    q := &m["a"]
    q.bal = 2
    if true {
        q, tag := &acct{bal: 5}, 1
        _ = q.bal
        _ = tag
    }
}
"#;
    let (verdict, codes) = check_codes(src);
    assert_eq!(verdict, Verdict::Accept, "expected Accept, got {:?}", codes);
}
