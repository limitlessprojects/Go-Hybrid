//! issue_2945_single_var_map_key_soundness.rs — SPEC-LANGUAGE §1.3 LX13 single-var map loop keys
//!
//! Issue #2945: `for x in e` over a map binds the KEY per LX13 ("for x in e iterates
//! slices, arrays, strings (runes), maps (keys), channels (until close), sets (elements)").
//! Previously, hy-check and hy-emit bound the VALUE instead.

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
fn single_var_map_iteration_binds_key_type() {
    // Repro 1: correct program binding string key to string variable
    let valid_src = r#"
fn main() {
    m := map[string]int{"a": 1}
    for k in m {
        var s string = k
        fmt.println(s)
    }
}
"#;
    let r = check(valid_src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "single-var map iterate must bind key type (string), got diags: {:?}",
        r.diagnostics
    );
}

#[test]
fn single_var_map_iteration_rejects_binding_as_value_type() {
    // Repro 2: wrong program attempting to bind string key to int variable
    let invalid_src = r#"
fn main() {
    m := map[string]int{"a": 1}
    for k in m {
        var s int = k
        fmt.println(s)
    }
}
"#;
    let r = check(invalid_src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "single-var map iterate must reject binding key (string) into int variable"
    );
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for type mismatch, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn two_var_map_iteration_binds_key_and_value() {
    let valid_two_vars = r#"
fn main() {
    m := map[string]int{"a": 1}
    for k, v in m {
        var s string = k
        var i int = v
        fmt.println(s, i)
    }
}
"#;
    let r = check(valid_two_vars);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "two-var map iterate must bind key (string) and value (int), got: {:?}",
        r.diagnostics
    );
}
