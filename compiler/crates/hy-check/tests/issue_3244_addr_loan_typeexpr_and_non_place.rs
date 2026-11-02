//! issue_3244_addr_loan_typeexpr_and_non_place.rs — regression tests for Issue #3244:
//! TypeExpr array-length expressions, nested borrows in returned &m[k], and non-place map borrows.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("snippet must parse");
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
fn test_issue_3244_type_expr_local_array_length_slot_loan_projection_rejects() {
    let src = r#"
package main

struct acct { bal int }
fn f(p *int) int {
    _ = p
    return 1
}

fn main() {
    var m map[string]acct
    n := &m["a"]
    var a [f(&n.bal)]int
    _ = a
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(res.diagnostics.iter().any(|d| d.message.contains("takes the address of a loan")));
}

#[test]
fn test_issue_3244_type_expr_slice_lit_array_length_slot_loan_projection_rejects() {
    let src = r#"
package main

struct acct { bal int }
fn f(p *int) int {
    _ = p
    return 1
}

fn main() {
    var m map[string]acct
    n := &m["a"]
    _ = [f(&n.bal)]int{1}
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(res.diagnostics.iter().any(|d| d.message.contains("takes the address of a loan")));
}

#[test]
fn test_issue_3244_type_expr_map_lit_key_array_length_slot_loan_projection_rejects() {
    let src = r#"
package main

struct acct { bal int }
fn f(p *int) int {
    _ = p
    return 1
}

fn main() {
    var m map[string]acct
    n := &m["a"]
    _ = map[[f(&n.bal)]int]int{}
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(res.diagnostics.iter().any(|d| d.message.contains("takes the address of a loan")));
}

#[test]
fn test_issue_3244_type_expr_fn_lit_param_array_length_slot_loan_projection_rejects() {
    let src = r#"
package main

struct acct { bal int }
fn f(p *int) int {
    _ = p
    return 1
}

fn main() {
    var m map[string]acct
    n := &m["a"]
    _ = fn(x [f(&n.bal)]int) {}
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(res.diagnostics.iter().any(|d| d.message.contains("takes the address of a loan")));
}

#[test]
fn test_issue_3244_type_expr_local_array_length_map_slot_borrow_rejects() {
    let src = r#"
package main

struct acct { bal int }
fn f(p *acct) int {
    _ = p
    return 1
}

fn main() {
    var m map[string]acct
    var a [f(&m["a"])]int
    _ = a
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(error_codes(&res).contains(&"HYB-E3104".to_string()));
}

#[test]
fn test_issue_3244_return_nested_slot_loan_projection_in_map_index_rejects() {
    let src = r#"
package main

struct acct { bal int }
fn k(p *int) string {
    _ = p
    return "a"
}

fn get(m map[string]acct) *acct {
    n := &m["a"]
    return &m[k(&n.bal)]
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(res.diagnostics.iter().any(|d| d.message.contains("takes the address of a loan")));
}

#[test]
fn test_issue_3244_non_place_map_base_local_define_rejects() {
    let src = r#"
package main

struct acct { bal int }
fn get_map() map[string]acct {
    return map[string]acct{"a": acct{bal: 1}}
}

fn main() {
    r := &get_map()["a"]
    _ = r
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(error_codes(&res).contains(&"HYB-E3104".to_string()));
}

#[test]
fn test_issue_3244_non_place_map_literal_base_local_define_rejects() {
    let src = r#"
package main

struct acct { bal int }

fn main() {
    r := &(map[string]acct{"a": acct{bal: 1}}["a"])
    _ = r
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(error_codes(&res).contains(&"HYB-E3104".to_string()));
}

#[test]
fn test_issue_3244_non_place_map_base_assign_rejects() {
    let src = r#"
package main

struct acct { bal int }
fn get_map() map[string]acct {
    return map[string]acct{"a": acct{bal: 1}}
}

fn main() {
    var r *acct
    r = &get_map()["a"]
    _ = r
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(error_codes(&res).contains(&"HYB-E3104".to_string()));
}

#[test]
fn test_issue_3244_place_map_slot_loan_valid_accepts() {
    let src = r#"
package main

struct acct { bal int }

fn main() {
    var m map[string]acct
    n := &m["a"]
    n.bal = 42
    _ = n
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Accept);
    assert_eq!(error_codes(&res), Vec::<String>::new());
}
