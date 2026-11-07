//! Issue #4036: the map literal KEY slot is a type contract too.
//!
//! `check_composite_nils` (sendability.rs) checked only the map *value* against
//! its shape and never `is_nil_literal(k)` against the *key* shape. That let
//! `map[int]int{nil: 1}` pass `hyb check` with 0 diagnostics while `go build`
//! refuses it ("cannot use nil as int value in map literal") — an
//! accepted-implies-buildable break, and a miss of the E2001/N1 type contract.
//!
//! What this file pins:
//!   1. a `nil` key in a non-nilable key slot is rejected with exactly ONE
//!      diagnostic (no double-fire from the key sweep plus the recursion);
//!   2. the same literal with a valid key stays accepted, so the new sweep
//!      does not turn into a blanket rejection of map literals;
//!   3. the value-slot rule that already existed still fires once, so the key
//!      sweep did not disturb it.

use hy_check::{check_file, CheckResult};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn nil_slot_errors(r: &CheckResult) -> Vec<&hy_diag::Diagnostic> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error && d.code == "HYB-E4001")
        .collect()
}

#[test]
fn nil_map_key_in_non_nilable_slot_rejected() {
    // The exact repro from #4036. Before the fix this printed
    // "OK (0 diagnostics)".
    let r = check(
        r#"
fn bad() {
    m := map[int]int{nil: 1}
    _ = m
}
"#,
    );
    let errs = nil_slot_errors(&r);
    assert_eq!(
        errs.len(),
        1,
        "expected exactly one E4001 for the nil map key, got {}: {:?}",
        errs.len(),
        errs.iter().map(|d| d.message.clone()).collect::<Vec<_>>()
    );
    let msg = &errs[0].message;
    assert!(
        msg.contains("map key slot"),
        "the diagnostic must name the key slot, got: {msg}"
    );
}

#[test]
fn nil_map_key_in_string_slot_rejected() {
    // The same gap with a string key: `map[string]int{nil: 1}`.
    let r = check(
        r#"
fn bad() {
    m := map[string]int{nil: 1}
    _ = m
}
"#,
    );
    let errs = nil_slot_errors(&r);
    assert_eq!(errs.len(), 1, "expected one E4001, got {}", errs.len());
    assert!(errs[0].message.contains("map key slot"));
}

#[test]
fn valid_map_key_still_accepted() {
    // Guard against over-rejection: an ordinary key must stay clean.
    let r = check(
        r#"
fn good() {
    m := map[int]int{1: 1}
    _ = m
}
"#,
    );
    let errs = nil_slot_errors(&r);
    assert!(
        errs.is_empty(),
        "a valid key must not be reported: {:?}",
        errs.iter().map(|d| d.message.clone()).collect::<Vec<_>>()
    );
}

#[test]
fn nil_map_value_slot_rule_unchanged() {
    // The pre-existing value-slot rule must still fire once and only once.
    let r = check(
        r#"
fn bad() {
    m := map[string]*int{"a": nil}
    _ = m
}
"#,
    );
    let errs = nil_slot_errors(&r);
    assert_eq!(
        errs.len(),
        1,
        "the value-slot rule must fire exactly once, got {}",
        errs.len()
    );
    assert!(
        errs[0].message.contains("map value slot"),
        "got: {}",
        errs[0].message
    );
}
