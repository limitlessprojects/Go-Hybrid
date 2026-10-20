//! Regression tests for issue #763: HYB-E2010 must reject unaddressable
//! map-element field/array stores and O8 slot loans inside closures (closure
//! map parameters, closure-local maps, and pre-shadow outer maps), across
//! generic struct projections (`map[K]box[acct]`, `bank[acct]`), and on
//! `move`-wrapped slot loans (`q := move &m[k].f`).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn rejects_e2010(src: &str) {
    let r = check(src);
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error && d.code == "HYB-E2010"),
        "expected HYB-E2010, got {:?}\nsrc:\n{}",
        r.diagnostics,
        src
    );
    assert_eq!(r.verdict, Verdict::Reject, "src:\n{}", src);
}

#[test]
fn closure_map_param_and_local_reject_e2010() {
    rejects_e2010(
        r#"struct acct {
    bal int
}

fn main() {
    g := fn(m map[string]acct) {
        m["a"].bal = 9
    }
    g(map[string]acct{"a": acct{bal: 1}})
}
"#,
    );

    rejects_e2010(
        r#"struct acct {
    bal int
}

fn main() {
    f := fn() {
        m := map[string]acct{"a": acct{bal: 1}}
        m["a"].bal = 9
    }
    f()
}
"#,
    );
}

#[test]
fn generic_struct_and_move_slot_loan_reject_e2010() {
    rejects_e2010(
        r#"struct acct {
    bal int
}

struct box[t] {
    val t
}

fn main() {
    m := map[string]box[acct]{"a": box[acct]{val: acct{bal: 1}}}
    m["a"].val.bal = 9
}
"#,
    );

    rejects_e2010(
        r#"struct acct {
    bal int
}

struct bank[t] {
    accts map[string]t
}

fn main() {
    b := bank[acct]{accts: map[string]acct{"a": acct{bal: 1}}}
    b.accts["a"].bal = 9
}
"#,
    );

    rejects_e2010(
        r#"struct acct {
    bal int
}

fn main() {
    m := map[string]acct{"a": acct{bal: 1}}
    q := move &m["a"].bal
    _ = q
}
"#,
    );
}
