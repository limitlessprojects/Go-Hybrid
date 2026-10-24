//! slot_loan_rebind_bypass.rs — regression tests for issue #1431:
//! hy-check: O8 map-slot loan rebinds via multi-return, closure bodies, and
//! post-value-update address-of bypass HYB-E3104.

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn has_diag(res: &hy_check::CheckResult, code: &str) -> bool {
    res.diagnostics.iter().any(|d| d.code == code && d.severity == Severity::Error)
}

#[test]
fn multireturn_assign_rebind_to_pointer_rejects_e3104() {
    let src = r#"
package main

import "fmt"

struct acct {
    bal int
}

fn next_acct() (*acct, bool) {
    a := new(acct)
    return a, true
}

fn main() {
    m := map[string]acct{"a": acct{bal: 1}}
    q := &m["a"]
    var ok bool
    q, ok = next_acct()
    fmt.println("{ok}")
}
"#;
    let res = check(src);
    assert!(
        has_diag(&res, "HYB-E3104"),
        "expected HYB-E3104 when multi-return assign re-binds map-slot loan to pointer, got: {:?}",
        res.diagnostics
    );
}

#[test]
fn multireturn_short_define_rebind_to_pointer_rejects_e3104() {
    let src = r#"
package main

import "fmt"

struct acct {
    bal int
}

fn next_acct() (*acct, bool) {
    a := new(acct)
    return a, true
}

fn main() {
    m := map[string]acct{"a": acct{bal: 1}}
    q := &m["a"]
    q, ok := next_acct()
    fmt.println("{ok}")
}
"#;
    let res = check(src);
    assert!(
        has_diag(&res, "HYB-E3104"),
        "expected HYB-E3104 when multi-return short define re-binds map-slot loan to pointer, got: {:?}",
        res.diagnostics
    );
}

#[test]
fn closure_body_rebind_to_pointer_rejects_e3104() {
    let src = r#"
package main

import "fmt"

struct acct {
    bal int
}

fn main() {
    f := fn() {
        m2 := map[string]acct{"a": acct{bal: 1}}
        q2 := &m2["a"]
        q2 = &acct{bal: 5}
        fmt.println("{q2.bal}")
    }
    f()
}
"#;
    let res = check(src);
    assert!(
        has_diag(&res, "HYB-E3104"),
        "expected HYB-E3104 when closure body re-binds map-slot loan to pointer, got: {:?}",
        res.diagnostics
    );
}

#[test]
fn post_value_update_addrof_binding_rejects_e3104() {
    let src = r#"
package main

import "fmt"

struct acct {
    bal int
}

fn main() {
    m3 := map[string]acct{"a": acct{bal: 1}}
    var q3 acct
    q3 = &m3["a"]
    q3 = acct{bal: 2}
    p := &q3
    p.bal = 99
    fmt.println("{p.bal}")
}
"#;
    let res = check(src);
    assert!(
        has_diag(&res, "HYB-E3104"),
        "expected HYB-E3104 when taking address-of slot loan after whole-value update, got: {:?}",
        res.diagnostics
    );
}

#[test]
fn post_value_update_addrof_field_projection_rejects_e3104() {
    let src = r#"
package main

import "fmt"

struct acct {
    bal int
}

fn main() {
    m3 := map[string]acct{"a": acct{bal: 1}}
    var q3 acct
    q3 = &m3["a"]
    q3 = acct{bal: 2}
    p := &q3.bal
    fmt.println("{p}")
}
"#;
    let res = check(src);
    assert!(
        has_diag(&res, "HYB-E3104"),
        "expected HYB-E3104 when taking address-of field projection after whole-value update, got: {:?}",
        res.diagnostics
    );
}

#[test]
fn ordinary_variable_not_slot_loan_stays_accepted() {
    let src = r#"
package main

import "fmt"

struct acct {
    bal int
}

fn next_acct() (*acct, bool) {
    a := new(acct)
    return a, true
}

fn main() {
    var q acct
    q = acct{bal: 2}
    p := &q
    p.bal = 99
    var ptr *acct
    var ok bool
    ptr, ok = next_acct()
    fmt.println("{q.bal} {ok}")
}
"#;
    let res = check(src);
    assert!(
        !has_diag(&res, "HYB-E3104"),
        "expected no HYB-E3104 for ordinary non-loan variables, got: {:?}",
        res.diagnostics
    );
}

#[test]
fn rebind_to_another_slot_loan_stays_accepted() {
    let src = r#"
package main

import "fmt"

struct acct {
    bal int
}

fn main() {
    m := map[string]acct{"a": acct{bal: 1}, "b": acct{bal: 2}}
    q := &m["a"]
    q.bal = 10
    q = &m["b"]
    q.bal = 20
    a := m["a"]
    b := m["b"]
    fmt.println("{a.bal} {b.bal}")
}
"#;
    let res = check(src);
    assert!(
        !has_diag(&res, "HYB-E3104"),
        "expected no HYB-E3104 when re-binding a slot loan to another slot loan, got: {:?}",
        res.diagnostics
    );
}
