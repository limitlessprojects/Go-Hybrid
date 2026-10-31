//! issue_1555_parenthesized_field_and_nested_index_callees.rs
//!
//! Regression test for issue #1555:
//! `hy-check`: `(undefined_var).field` and nested index expressions in folded
//! call callees bypass `HYB-E1001`.
//!
//! 1. `Expr::Field` must only treat bare `Expr::Ident` bases as package prefix
//!    candidates; parenthesized bases like `(undefined_var).field` or `(fmt).println`
//!    can never be valid package qualifiers in Go, and must evaluate the base in
//!    expression position, rejecting undefined names with `HYB-E1001`.
//! 2. `Scan::ident` in folded type args / indexed callees (`fns[...](arg)`) must
//!    lex and resolve all operand identifiers in nested index expressions such
//!    as `fns[idx_map[missing_key]](1)`.

use hy_check::{check_file, CheckResult};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let f = parse_file("test.hy", src).expect("test source must parse");
    check_file(&f)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn parenthesized_undefined_field_base_rejects_e1001() {
    let src = r#"package main

fn main() {
    _ = (undefined_var).field
}
"#;
    let res = check(src);
    let codes = error_codes(&res);
    assert!(
        codes.iter().any(|c| c == "HYB-E1001"),
        "expected HYB-E1001 on (undefined_var).field, got: {:?}",
        res.diagnostics
    );
    assert!(
        res.diagnostics
            .iter()
            .any(|d| d.message.contains("undefined_var")),
        "diagnostic must name `undefined_var`: {:?}",
        res.diagnostics
    );
}

#[test]
fn parenthesized_package_qualifier_rejects_e1001() {
    let src = r#"package main

fn main() {
    _ = (fmt).println
}
"#;
    let res = check(src);
    let codes = error_codes(&res);
    assert!(
        codes.iter().any(|c| c == "HYB-E1001"),
        "parenthesized package qualifier (fmt).println must reject with HYB-E1001, got: {:?}",
        res.diagnostics
    );
}

#[test]
fn nested_index_in_callee_rejects_undefined_names() {
    // Both idx_map and missing_key undefined
    let src_both = r#"package main

fn main() {
    fns := map[int]fn(int){0: fn(x int) {}}
    fns[idx_map[missing_key]](1)
}
"#;
    let res_both = check(src_both);
    let codes_both = error_codes(&res_both);
    assert!(
        codes_both.iter().any(|c| c == "HYB-E1001"),
        "expected HYB-E1001 for fns[idx_map[missing_key]](1), got: {:?}",
        res_both.diagnostics
    );
    assert!(
        res_both
            .diagnostics
            .iter()
            .any(|d| d.message.contains("idx_map") || d.message.contains("missing_key")),
        "diagnostic must name missing nested index operand: {:?}",
        res_both.diagnostics
    );

    // idx_map defined, missing_key undefined
    let src_missing_key = r#"package main

fn main() {
    fns := map[int]fn(int){0: fn(x int) {}}
    idx_map := map[string]int{"k": 0}
    fns[idx_map[missing_key]](1)
}
"#;
    let res_key = check(src_missing_key);
    assert!(
        res_key
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E1001" && d.message.contains("missing_key")),
        "expected HYB-E1001 for missing_key, got: {:?}",
        res_key.diagnostics
    );

    // idx_map undefined, k defined
    let src_missing_map = r#"package main

fn main() {
    fns := map[int]fn(int){0: fn(x int) {}}
    k := "k"
    fns[idx_map[k]](1)
}
"#;
    let res_map = check(src_missing_map);
    assert!(
        res_map
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E1001" && d.message.contains("idx_map")),
        "expected HYB-E1001 for missing idx_map, got: {:?}",
        res_map.diagnostics
    );
}

#[test]
fn positive_controls_legal_parenthesized_field_and_nested_indexing_accept() {
    let src = r#"package main

struct point {
    x int
}

fn main() {
    p := point{x: 42}
    _ = (p).x
    _ = ((p)).x

    fns := map[int]fn(int){0: fn(x int) {}}
    idx_map := map[string]int{"k": 0}
    k := "k"
    fns[idx_map[k]](42)

    fmt.println("ok")
}
"#;
    let res = check(src);
    assert!(
        res.diagnostics.is_empty(),
        "expected 0 diagnostics on legal controls, got: {:?}",
        res.diagnostics
    );
}
