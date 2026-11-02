//! blank_identifier_value_position_2710.rs — issue #2710 (Row 13 BUILDABLE).
//!
//! The blank identifier `_` discards a value; it is not a value. Go refuses to
//! read it (`cannot use _ as value or type`), yet `hyb check` accepted `_` in
//! every expression value position (`_ = _`, `x := _`, `foo(_)`, `_ + 1`,
//! `[]int{_}`, `map[string]int{"a": _}`) with 0 diagnostics. `hy-check` must
//! reject each with `HYB-E1002`, while leaving the legal discard targets
//! (`_ = v`, `x, _ = f()`) accepted.

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
fn blank_on_rhs_of_assign_rejects_e1002() {
    let src = "\
fn main() {
    _ = _
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "{:?}",
        r.diagnostics
    );
}

#[test]
fn blank_in_short_define_rejects_e1002() {
    let src = "\
fn main() {
    x := _
    _ = x
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "{:?}",
        r.diagnostics
    );
}

#[test]
fn blank_as_call_argument_rejects_e1002() {
    let src = "\
fn foo(x int) {}
fn main() {
    foo(_)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "{:?}",
        r.diagnostics
    );
}

#[test]
fn blank_in_arithmetic_rejects_e1002() {
    let src = "\
fn main() {
    y := _ + 1
    _ = y
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "{:?}",
        r.diagnostics
    );
}

#[test]
fn blank_in_slice_literal_rejects_e1002() {
    let src = "\
fn main() {
    xs := []int{_}
    _ = xs
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "{:?}",
        r.diagnostics
    );
}

#[test]
fn blank_in_map_literal_rejects_e1002() {
    let src = "\
fn main() {
    m := map[string]int{\"a\": _}
    _ = m
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "{:?}",
        r.diagnostics
    );
}

#[test]
fn blank_discard_targets_stay_accepted() {
    let src = "\
fn foo() int {
    return 1
}
fn main() {
    _ = 5
    _ = foo()
    _ = foo() + 1
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(
        error_codes(&r).is_empty(),
        "expected 0 errors, got {:?}",
        r.diagnostics
    );
}

#[test]
fn blank_in_multi_target_assignment_stays_accepted() {
    let src = "\
fn two() (int, int) {
    return 1, 2
}
fn main() {
    var x int
    x, _ = two()
    _ = x
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(
        error_codes(&r).is_empty(),
        "expected 0 errors, got {:?}",
        r.diagnostics
    );
}

#[test]
fn e3_error_return_blank_zero_value_placeholder_stays_accepted() {
    let src = "\
fn validate_email(ok bool, s string) (string, error) {
    if !ok {
        return _, errors.new(\"invalid email address\")
    }
    return s, nil
}
fn main() {}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    assert!(
        error_codes(&r).is_empty(),
        "expected 0 errors for SPEC E3 `return _, err`, got {:?}",
        r.diagnostics
    );
}

#[test]
fn blank_in_non_error_return_rejects_e1002() {
    let src = "\
fn foo() int {
    return _
}
fn main() {}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "{:?}",
        r.diagnostics
    );
}
