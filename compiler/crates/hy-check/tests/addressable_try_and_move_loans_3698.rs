//! Regression tests for #3698: `?`-propagated call results (`&(make_pair()?).x`,
//! `&(make_arr()?)[0]`) and `move`-wrapped unaddressable slot loans
//! (`move &m["a"].bal`, `&(move m["a"]).bal`, `&(move m["a"].arr[0])`) must
//! reject as HYB-E2010.

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
fn rejects_try_propagated_call_field_and_array_index_loans_3698() {
    rejects_with(
        "struct pair {\n\
            x int\n\
        }\n\
        fn make_pair() (pair, error) {\n\
            return pair{x: 1}, nil\n\
        }\n\
        fn make_arr() ([2]int, error) {\n\
            var arr [2]int\n\
            return arr, nil\n\
        }\n\
        fn main() {\n\
            n1 := &(make_pair()?).x\n\
            n2 := &(make_arr()?)[0]\n\
            _ = n1\n\
            _ = n2\n\
        }\n",
        "HYB-E2010",
    );
}

#[test]
fn rejects_move_wrapped_map_sub_projection_loans_3698() {
    rejects_with(
        "struct acct {\n\
            bal int\n\
            arr [2]int\n\
        }\n\
        fn main() {\n\
            m := map[string]acct{\"a\": acct{bal: 1}}\n\
            n1 := move &m[\"a\"].bal\n\
            n2 := &(move m[\"a\"]).bal\n\
            n3 := &(move m[\"a\"].arr[0])\n\
            _ = n1\n\
            _ = n2\n\
            _ = n3\n\
            _ = m\n\
        }\n",
        "HYB-E2010",
    );
    accepts(
        "struct acct {\n\
            bal int\n\
        }\n\
        fn main() {\n\
            m := map[string]acct{\"a\": acct{bal: 1}}\n\
            n := &(m[\"a\"])\n\
            n.bal = 2\n\
            _ = m\n\
        }\n",
    );
}
