//! issue_2616_closure_slot_loan_uniqueness.rs — regression tests for Issue #2616:
//! verification that check_closure_slot_loans enforces O8 slot-loan uniqueness
//! and slot field-store conflict checks (HYB-E3102) inside closure bodies.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn second_slot_loan_in_closure_rejected_with_e3102() {
    let src = "\
struct acct { bal int }

fn main() {
    m := map[string]acct{\"a\": acct{bal: 1}}
    f := fn() {
        p := &m[\"a\"]
        q := &m[\"a\"]
        q.bal = 50
        p.bal = p.bal + 1
    }
    f()
}
";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected reject for second live slot loan in closure"
    );
    assert!(
        error_codes(&r).contains(&"HYB-E3102".to_string()),
        "expected HYB-E3102, got diags: {:?}",
        r.diagnostics
    );
}

#[test]
fn field_store_on_loaned_slot_in_closure_rejected_with_e3102() {
    let src = "\
struct acct { bal int }

fn main() {
    ns := []acct{acct{bal: 1}}
    f := fn() {
        s := &ns[0]
        ns[0].bal += 50
        s.bal = s.bal + 1
    }
    f()
}
";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected reject for field store on loaned slot in closure"
    );
    assert!(
        error_codes(&r).contains(&"HYB-E3102".to_string()),
        "expected HYB-E3102, got diags: {:?}",
        r.diagnostics
    );
}

#[test]
fn distinct_literal_slots_in_closure_stay_accepted() {
    let src = "\
struct acct { bal int }

fn main() {
    ns := []acct{acct{bal: 1}, acct{bal: 2}}
    f := fn() {
        x := &ns[0]
        ns[1].bal = 9
        _ = x.bal
    }
    f()
}
";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept for distinct literal slots in closure, got diags: {:?}",
        r.diagnostics
    );
}
