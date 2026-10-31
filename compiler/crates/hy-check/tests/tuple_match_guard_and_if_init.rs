//! Regression tests for Issue #1905 (PR #1907):
//! T23 (`HYB-E2002`) enforcement in `Stmt::Match` arm guards with `some(b)` bindings
//! and closure `Stmt::If` conditions with `init` bindings, without double-firing
//! `check_stmt_multi_value` on `init` statements already visited by Pass B / `closures.rs`.

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
        "expected exactly 1 HYB-E2002 diagnostic (no double-firing), got {} in {:?} for:\n{src}",
        e2002.len(),
        r.diagnostics
    );
}

fn assert_accept(src: &str) {
    let r = check_hy(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected Accept for:\n{src}\ngot: {:?}",
        r.diagnostics
    );
    assert!(
        r.diagnostics.is_empty(),
        "expected 0 diagnostics, got {:?}",
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

#[test]
fn closure_if_init_short_decl_multi_value_fn_in_cond_rejected() {
    assert_single_e2002(
        r#"
fn single() int { return 1 }
fn pair() (int, int) { return 1, 2 }
fn main() {
    f := single
    cb := fn() {
        if f := pair; f() == 1 {}
    }
    cb()
}
"#,
    );
}

#[test]
fn closure_if_init_multi_binding_shadows_single_value_fn_rejected() {
    assert_single_e2002(
        r#"
type single_fn = fn() int
type pair_fn = fn() int int
const _anchor = 0
fn single() int { return 1 }
fn pair() (int, int) { return 1, 2 }
fn both_fns() (single_fn, pair_fn) { return single, pair }
fn main() {
    f := single
    cb := fn() {
        if g, f := both_fns(); g() == f() {}
    }
    cb()
}
"#,
    );
}

#[test]
fn if_init_arity_mismatch_emits_exactly_one_diagnostic_not_duplicated() {
    // Regression guard: `Stmt::If` in `check_stmt_multi_value` must NOT re-run
    // `check_stmt_multi_value(st, init)` because Pass B (`lib.rs`) and
    // `check_stmt_boundaries` (`closures.rs`) already visit `init` as its own
    // statement. Re-running it inside `Stmt::If` would emit duplicate HYB-E2002.
    assert_single_e2002(
        r#"
fn pair() (int, int) { return 1, 2 }
fn main() {
    if x := pair(); x == 1 {}
}
"#,
    );
    assert_single_e2002(
        r#"
fn pair() (int, int) { return 1, 2 }
fn main() {
    cb := fn() {
        if x := pair(); x == 1 {}
    }
    cb()
}
"#,
    );
}

#[test]
fn positive_controls_shadowing_multi_value_with_single_value_accepted() {
    assert_accept(
        r#"
fn single() int { return 1 }
fn pair() (int, int) { return 1, 2 }
fn main() {
    f := pair
    opt := some(single)
    match opt {
        some(f) if f() == 1: {}
        some(_): {}
        none: {}
    }
    cb := fn() {
        if f := single; f() == 1 {}
    }
    cb()
    _ = f
}
"#,
    );
}
