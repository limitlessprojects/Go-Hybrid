//! Regression tests for Issue #1905: T23 (`HYB-E2002`) enforcement in
//! `Stmt::Match` arm guards with `some(b)` bindings.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check_hy(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("source must parse");
    check_file(&ast)
}

fn assert_single_e2002(src: &str) {
    let r = check_hy(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected Reject for:\n{src}\ngot: {:?}",
        r.diagnostics
    );
    let e2002: Vec<_> = r
        .diagnostics
        .iter()
        .filter(|d| d.code == "HYB-E2002")
        .collect();
    assert_eq!(
        e2002.len(),
        1,
        "expected exactly 1 HYB-E2002 diagnostic, got {} in {:?}",
        e2002.len(),
        r.diagnostics
    );
}

#[test]
fn match_guard_some_binding_direct_multi_value_fn_rejected() {
    assert_single_e2002(
        r#"
fn pair() (int, int) { return 1, 2 }
fn main() {
    opt := some(pair)
    match opt {
        some(f) if f() == 1: {}
        none: {}
    }
}
"#,
    );
}

#[test]
fn match_guard_some_binding_shadows_outer_single_value_fn_rejected() {
    assert_single_e2002(
        r#"
fn single() int { return 1 }
fn pair() (int, int) { return 1, 2 }
fn main() {
    f := single
    opt := some(pair)
    match opt {
        some(f) if f() == 1: {}
        none: {}
    }
}
"#,
    );
}

#[test]
fn match_guard_some_binding_via_type_alias_subject_rejected() {
    assert_single_e2002(
        r#"
type pair_fn = fn() int int
type opt_pair_fn = option[pair_fn]
fn pair() (int, int) { return 1, 2 }
fn make_opt() opt_pair_fn { return some(pair) }
fn main() {
    opt := make_opt()
    match opt {
        some(f) if f() == 1: {}
        some(_): {}
        none: {}
    }
}
"#,
    );
}

#[test]
fn match_guard_inside_closure_some_binding_multi_value_rejected() {
    assert_single_e2002(
        r#"
fn single() int { return 1 }
fn pair() (int, int) { return 1, 2 }
fn main() {
    f := single
    opt := some(pair)
    cb := fn() {
        match opt {
            some(f) if f() == 1: {}
            none: {}
        }
    }
    cb()
}
"#,
    );
}
