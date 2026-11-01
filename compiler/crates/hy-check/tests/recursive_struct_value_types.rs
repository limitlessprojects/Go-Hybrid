//! recursive_struct_value_types.rs — tests for HYB-E1002 rejection
//! of recursive struct value types (Issue #1566: Row 13 BUILDABLE).

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
        .map(|d| d.code.to_string())
        .collect()
}

#[test]
fn direct_recursive_struct_value_is_rejected() {
    let src = r#"
struct node {
    next node
}

fn main() {
    var n node
    _ = n
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        res.diagnostics
    );
    assert!(
        res.diagnostics.iter().any(|d| d.message.contains("invalid recursive type: node refers to itself")),
        "diagnostic message should match Go's wording: {:?}",
        res.diagnostics
    );
    assert!(
        res.diagnostics.iter().any(|d| d.notes.iter().any(|n| n.contains("cycle: node -> node"))),
        "notes should show the cycle path: {:?}",
        res.diagnostics
    );
}

#[test]
fn array_recursive_struct_value_is_rejected() {
    let src = r#"
struct node {
    kids [2]node
}

fn main() {
    var n node
    _ = n
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        res.diagnostics
    );
    assert!(
        res.diagnostics.iter().any(|d| d.message.contains("invalid recursive type: node refers to itself")),
        "diagnostic message should match Go's wording: {:?}",
        res.diagnostics
    );
}

#[test]
fn multidimensional_array_recursive_struct_value_is_rejected() {
    let src = r#"
struct grid_node {
    cells [2][3]grid_node
}

fn main() {
    var g grid_node
    _ = g
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        res.diagnostics
    );
}

#[test]
fn indirect_mutual_recursive_struct_value_is_rejected() {
    let src = r#"
struct a {
    b b
}

struct b {
    a a
}

fn main() {
    var x a
    _ = x
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        res.diagnostics
    );
    assert!(
        res.diagnostics.iter().any(|d| d.message.contains("invalid recursive type")),
        "diagnostic should flag mutual recursive type: {:?}",
        res.diagnostics
    );
}

#[test]
fn type_alias_recursive_struct_value_is_rejected() {
    let src = r#"
type my_node = node

struct node {
    next my_node
}

fn main() {
    var n node
    _ = n
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for aliased cycle, got {:?}",
        res.diagnostics
    );
}

#[test]
fn slice_indirection_breaks_cycle_is_accepted() {
    let src = r#"
struct node {
    kids []node
}

fn main() {
    var n node
    _ = n
}
"#;
    let res = check(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "slice indirection is legal Go and must be accepted: {:?}",
        res.diagnostics
    );
}

#[test]
fn map_and_chan_indirection_breaks_cycle_is_accepted() {
    let src = r#"
struct tree {
    subtrees map[string]tree
    ch chan tree
}

fn main() {
    var t tree
    _ = t
}
"#;
    let res = check(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "map and chan indirections are legal Go and must be accepted: {:?}",
        res.diagnostics
    );
}

#[test]
fn pointer_field_is_handled_by_e3103_not_duplicate_cycle() {
    let src = r#"
struct node {
    next *node
}

fn main() {
    var n node
    _ = n
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    // Should be HYB-E3103 (fields never hold borrows), without invalid recursive type
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E3103"),
        "expected HYB-E3103 for pointer field, got {:?}",
        res.diagnostics
    );
    assert!(
        !res.diagnostics.iter().any(|d| d.message.contains("invalid recursive type")),
        "should not duplicate recursive type diagnostic on pointer field: {:?}",
        res.diagnostics
    );
}
