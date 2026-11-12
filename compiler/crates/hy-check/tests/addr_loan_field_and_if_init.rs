//! Regression tests for `addr_loan.rs`:
//! 1. Struct-field O8 slot loans (`f := &s.field`, `f = &s.field`) are marked `slot_loan = true`,
//!    so taking `&f`, `&f.sub`, `move f`, or re-binding `f` to a general pointer rejects `HYB-E3104`.
//! 2. `Stmt::If { init: Some(i), cond, .. }` evaluates `i` before `cond`, so slot loans bound or
//!    re-bound in `i` are enforced in `cond` and across the `if` body.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn assert_e3104(src: &str) {
    let r = check_file(&parse_file("t.hy", src).expect("snippet parses"));
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected Reject for:\n{src}\ngot: {:?}",
        r.diagnostics
    );
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3104"),
        "expected HYB-E3104, got {:?} for:\n{src}",
        r.diagnostics
    );
}

fn assert_accept(src: &str) {
    let r = check_file(&parse_file("t.hy", src).expect("snippet parses"));
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected Accept, got {:?} for:\n{src}",
        r.diagnostics
    );
    assert!(
        r.diagnostics.is_empty(),
        "expected 0 diagnostics, got {:?}",
        r.diagnostics
    );
}

#[test]
fn struct_field_slot_loan_rejects_addr_of_move_and_general_ptr_rebind() {
    for src in [
        r#"struct acct { bal int }
fn main() {
    a := acct{bal: 1}
    f := &a.bal
    p := &f
    fmt.println(p)
}
"#,
        r#"struct inner { v int }
struct outer { sub inner }
fn main() {
    o := outer{sub: inner{v: 1}}
    f := &o.sub
    p := &f.v
    fmt.println(p)
}
"#,
        r#"struct inner { v int }
struct outer { sub inner }
fn main() {
    o := outer{sub: inner{v: 1}}
    other := inner{v: 2}
    f := &o.sub
    f = &other
    fmt.println(f.v)
}
"#,
        r#"struct inner { v int }
struct outer { sub inner }
fn take_ptr(p *inner) { _ = p.v }
fn main() {
    o := outer{sub: inner{v: 1}}
    var f *inner
    f = &o.sub
    task take_ptr(move f)
}
"#,
    ] {
        assert_e3104(src);
    }
}

#[test]
fn if_init_slot_loan_rejects_addr_of_in_cond_and_general_ptr_rebind() {
    for src in [
        r#"struct acct { bal int }
fn check_ptr(p *acct) bool { return p != nil }
fn main() {
    m := map[string]acct{"a": acct{bal: 1}}
    if n := &m["a"]; check_ptr(&n) {
        fmt.println(n.bal)
    }
}
"#,
        r#"struct acct { bal int }
fn check_int_ptr(p *int) bool { return p != nil }
fn main() {
    m := map[string]acct{"a": acct{bal: 1}}
    if n := &m["a"]; check_int_ptr(&n.bal) {
        fmt.println(n.bal)
    }
}
"#,
        r#"struct acct { bal int }
fn main() {
    f := fn() {
        m := map[string]acct{"a": acct{bal: 1}}
        if n := &m["a"]; true {
            p := &n.bal
            fmt.println(p)
        }
    }
    f()
}
"#,
    ] {
        assert_e3104(src);
    }
}

#[test]
fn struct_field_slot_loan_and_if_init_valid_controls_stay_accepted() {
    for src in [
        r#"struct inner { v int }
struct outer { sub inner }
fn main() {
    o := outer{sub: inner{v: 1}}
    f := &o.sub
    f.v = 9
    fmt.println(o.sub.v)
}
"#,
        r#"struct acct { bal int }
fn main() {
    m := map[string]acct{"a": acct{bal: 1}}
    if n := &m["a"]; n.bal > 0 {
        n.bal = 2
    }
    fmt.println(m["a"].bal)
}
"#,
    ] {
        assert_accept(src);
    }
}
