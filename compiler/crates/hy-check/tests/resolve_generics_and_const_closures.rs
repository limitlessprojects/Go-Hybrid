//! Regression tests for `HYB-E1001` (`resolve.rs`):
//! 1. Explicit generic function/type calls (`id[int](42)`, `pair[int, string](1, "a")`)
//!    and indexed function-map/slice calls (`m[k](42)`) where `parser.rs`
//!    (`fold_type_args`) folds `[…]` into `Expr::Ident` must not be falsely
//!    rejected with `HYB-E1001`, while `undefined_fn[int](42)` and
//!    `m[undef_key](42)` still reject fail-closed with `HYB-E1001`.
//! 2. Closures (`Expr::FnLit`) inside file-level `const` initializers or
//!    type array-length expressions must collect their parameters, named
//!    results, and local bindings without false `HYB-E1001` rejections and
//!    without leaking closure locals into other `const` declarations.
//! 3. Generic function signature types (`type_params`, receiver type params,
//!    and parameter names) must be in scope during `check_file_undefined_names`.

use hy_check::check_file;
use hy_syntax::parse_file;

fn check(src: &str) -> Vec<hy_diag::Diagnostic> {
    let f = parse_file("test.hy", src).expect("parse ok");
    check_file(&f).diagnostics
}

#[test]
fn explicit_generic_calls_and_indexed_fn_map_calls_do_not_falsely_reject_e1001() {
    let src = r#"
fn id[t any](x t) t {
    return x
}

fn pair[a any, b any](x a, y b) a {
    return x
}

fn apply_map(m map[string]fn(int) int, k string, v int) int {
    return m[k](v)
}

fn main() {
    a := id[int](42)
    b := pair[int, string](a, "ok")
    _ = b
}
"#;
    let diags = check(src);
    assert!(
        diags.is_empty(),
        "expected 0 diagnostics on valid explicit generic and indexed fn-map calls, got: {diags:?}"
    );
}

#[test]
fn undefined_generic_callee_and_undefined_fn_map_index_still_reject_e1001() {
    let src = r#"
fn apply_map(m map[string]fn(int) int) int {
    a := undefined_generic_fn[int](42)
    return m[undefined_key](a)
}

fn main() {}
"#;
    let diags = check(src);
    assert!(
        diags
            .iter()
            .any(|d| d.code == "HYB-E1001" && d.message.contains("undefined_generic_fn[int]")),
        "expected HYB-E1001 for undefined generic callee, got: {diags:?}"
    );
    assert!(
        diags
            .iter()
            .any(|d| d.code == "HYB-E1001" && d.message.contains("undefined_key")),
        "expected HYB-E1001 for undefined index in m[undefined_key](a), got: {diags:?}"
    );
}

#[test]
fn file_level_const_closure_collects_params_results_and_locals_without_leaking() {
    let valid = r#"
const add_one = fn(x int) (res int) {
    y := x + 1
    res = y
    return res
}

fn main() {}
"#;
    let diags = check(valid);
    assert!(
        !diags.iter().any(|d| d.code == "HYB-E1001"),
        "expected no false HYB-E1001 on file-level const closure params/results/locals, got: {diags:?}"
    );

    let invalid = r#"
const first_fn = fn(x int) int {
    local_inside_first := x + 1
    return local_inside_first + missing_inside_closure
}

const leaked_read = local_inside_first

fn main() {}
"#;
    let bad_diags = check(invalid);
    assert!(
        bad_diags
            .iter()
            .any(|d| d.code == "HYB-E1001" && d.message.contains("missing_inside_closure")),
        "expected HYB-E1001 on missing_inside_closure inside const closure, got: {bad_diags:?}"
    );
    assert!(
        bad_diags
            .iter()
            .any(|d| d.code == "HYB-E1001" && d.message.contains("local_inside_first")),
        "closure-local name in first_fn must not leak into subsequent const declarations: {bad_diags:?}"
    );
}

#[test]
fn fn_signature_types_see_fn_and_receiver_scope_in_file_level_check() {
    let src = r#"
struct box[n any] {
    v int
}

fn (b *box[n]) step(k int) [k]int {
    var out [4]int
    return out
}

fn main() {}
"#;
    let diags = check(src);
    assert!(
        !diags.iter().any(|d| d.code == "HYB-E1001"),
        "expected no HYB-E1001 on fn signature referencing param/receiver scope, got: {diags:?}"
    );
}
