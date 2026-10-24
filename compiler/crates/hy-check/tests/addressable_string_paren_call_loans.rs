//! addressable_string_paren_call_loans.rs — regression tests for issue #1343:
//! string-index assignment/loan (`s[0] = v`, `&s[0]`, including through `match`
//! option binders), parenthesized map sub-projections (`&(m[k].f)`), and
//! value-returning call projections (`&get_pair().x`, `&get_arr()[0]`) must
//! reject as HYB-E2010 (rule BUILDABLE).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects_with(src: &str, code: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == code),
        "expected {}, got {:?}\nsrc:\n{}",
        code,
        cs,
        src
    );
    assert_eq!(r.verdict, Verdict::Reject, "src:\n{}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    assert!(
        error_codes(&r).is_empty() && r.verdict == Verdict::Accept,
        "must accept, got {:?}\nsrc:\n{}",
        r.diagnostics,
        src
    );
}

#[test]
fn rejects_string_index_store_and_loan_including_match_option_binder() {
    rejects_with(
        "fn main() {
    s := "hello"
    s[0] = 65
    n := &s[0]
    _ = n
    _ = s
}
",
        "HYB-E2010",
    );
    rejects_with(
        "fn main() {
    var opt option[string] = some("hello")
    match opt {
        some(s): {
            s[0] = 65
            n := &s[0]
            _ = n
            _ = s
        }
        none: {}
    }
}
",
        "HYB-E2010",
    );
}

#[test]
fn rejects_paren_map_field_and_array_sub_projection_loans() {
    rejects_with(
        "struct acct {
    bal int
    arr [2]int
}
fn main() {
    m := map[string]acct{"a": acct{bal: 1}}
    n1 := &(m["a"].bal)
    n2 := &(((m["a"]).arr)[0])
    _ = n1
    _ = n2
    _ = m
}
",
        "HYB-E2010",
    );
    accepts(
        "struct acct {
    bal int
}
fn main() {
    m := map[string]acct{"a": acct{bal: 1}}
    n := &(m["a"])
    n.bal = 2
    _ = m
}
",
    );
}

#[test]
fn rejects_value_call_field_and_array_index_loans() {
    rejects_with(
        "struct pair {
    x int
}
fn get_pair() pair {
    return pair{x: 1}
}
fn get_arr() [2]int {
    var arr [2]int
    return arr
}
fn main() {
    n1 := &get_pair().x
    n2 := &get_arr()[0]
    _ = n1
    _ = n2
}
",
        "HYB-E2010",
    );
}
