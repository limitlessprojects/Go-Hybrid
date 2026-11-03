//! try_single_value_1960.rs — regression for issue #1960.
//!
//! `non_single_value_count` only understood `Expr::Call`, so `f()?`
//! (`Expr::Try`) was treated as producing exactly one value. A void fallible
//! function (`fn run() error`) yields zero values and a multi-value one
//! (`fn multi() (int, int, error)`) yields two after the trailing error is
//! unwrapped; both fail `go build` when used as a value. The fix unwraps the
//! Try operand and subtracts the trailing error result.

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_reject(src: &str) {
    let found = codes(src);
    assert!(
        found.iter().any(|code| code == "HYB-E1002"),
        "expected HYB-E1002, got {found:?} for:\n{src}"
    );
}

fn assert_accept(src: &str) {
    let found = codes(src);
    assert!(
        found.is_empty(),
        "expected acceptance, got {found:?} for:\n{src}"
    );
}

const VOID_AND_MULTI: &str = r#"fn run() error {
    return nil
}

fn multi() (int, int, error) {
    return 1, 2, nil
}

fn one() (int, error) {
    return 1, nil
}
"#;

// NOTE: `-run()?` (unary operand) is not covered here. `check_unary_operand`
// in buildable.rs does not call `check_single_value_position` at all; that
// wiring is issue #1850, not this helper. Once #1850 lands, the Try handling
// added here covers the unary shape automatically.

#[test]
fn rejects_void_try_as_slice_bound() {
    assert_reject(&format!(
        "{VOID_AND_MULTI}
fn f() error {{
    s := []int{{1, 2, 3}}
    sub := s[run()?:2]
    _ = sub
    return nil
}}
"
    ));
}

#[test]
fn rejects_void_try_as_array_index() {
    assert_reject(&format!(
        "{VOID_AND_MULTI}
fn f() error {{
    s := []int{{1, 2, 3}}
    x := s[run()?]
    _ = x
    return nil
}}
"
    ));
}

#[test]
fn rejects_multi_value_try_as_index() {
    assert_reject(&format!(
        "{VOID_AND_MULTI}
fn f() error {{
    s := []int{{1, 2, 3}}
    x := s[multi()?]
    _ = x
    return nil
}}
"
    ));
}

#[test]
fn rejects_void_try_as_call_argument() {
    assert_reject(&format!(
        "{VOID_AND_MULTI}
fn take(v int) {{ print(v) }}

fn f() error {{
    take(run()?)
    return nil
}}
"
    ));
}

#[test]
fn accepts_single_value_try_binding() {
    assert_accept(&format!(
        "{VOID_AND_MULTI}
fn f() error {{
    x := one()?
    _ = x
    return nil
}}
"
    ));
}

#[test]
fn accepts_multi_value_try_binding() {
    assert_accept(&format!(
        "{VOID_AND_MULTI}
fn f() error {{
    a, b := multi()?
    _ = a
    _ = b
    return nil
}}
"
    ));
}

#[test]
fn accepts_void_try_as_statement() {
    assert_accept(&format!(
        "{VOID_AND_MULTI}
fn f() error {{
    run()?
    return nil
}}
"
    ));
}
