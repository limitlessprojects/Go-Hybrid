//! issue_3180_call_return_scalar_array_widths.rs — regression tests for #3180:
//! verifying that function literal calls, multi-value arguments, and return statements
//! strictly enforce scalar width and fixed array length boundaries (Row 13 BUILDABLE, Al-Mizan).

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
fn direct_fn_lit_call_scalar_width_mismatch_rejects() {
    let src = "fn main() {\n    var x int64 = 42\n    (fn(a int32) {})(x)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for scalar width mismatch in func literal call"
    );
}

#[test]
fn direct_fn_lit_call_array_len_mismatch_rejects() {
    let src = "fn main() {\n    var arr [3]int\n    (fn(a [2]int) {})(arr)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for array length mismatch in func literal call"
    );
}

#[test]
fn direct_fn_lit_call_array_lit_mismatch_rejects() {
    let src = "fn main() {\n    (fn(a [2]int) {})([3]int{1, 2, 3})\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for array literal mismatch in func literal call"
    );
}

#[test]
fn multi_value_arg_scalar_mismatch_rejects() {
    let src = "fn pair() (int64, string) {\n    return 1, \"ok\"\n}\nfn foo(a int32, b string) {}\nfn main() {\n    foo(pair())\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for multi-value call scalar width mismatch"
    );
}

#[test]
fn return_statement_array_len_mismatch_var_rejects() {
    let src = "fn get_arr() [2]int {\n    var a [3]int\n    return a\n}\nfn main() {}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for return statement array length mismatch (variable)"
    );
}

#[test]
fn return_statement_array_len_mismatch_lit_rejects() {
    let src = "fn get_arr_lit() [2]int {\n    return [3]int{1, 2, 3}\n}\nfn main() {}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for return statement array length mismatch (literal)"
    );
}

#[test]
fn valid_fn_lit_and_return_shapes_accept() {
    let src = r#"
fn get_arr() [2]int {
    return [2]int{1, 2}
}

fn pair() (int32, string) {
    return 1, "ok"
}

fn foo(a int32, b string) {}

fn main() {
    (fn(a int32) {})(42)
    var arr [2]int
    (fn(a [2]int) {})(arr)
    (fn(a [2]int) {})([2]int{1, 2})
    foo(pair())
    _ = get_arr()
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "valid array lengths, matching scalars, and multi-value calls must accept cleanly"
    );
    assert!(
        error_codes(&r).is_empty(),
        "no errors expected for valid positive controls"
    );
}
