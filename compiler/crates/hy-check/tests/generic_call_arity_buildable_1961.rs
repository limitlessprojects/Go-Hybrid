//! Regression tests for #1961: calls to generic user functions failed open
//! end to end. `check_user_fn_arity` bailed on `!f.type_params.is_empty()`
//! and `check_user_fn_arg_types` did the same, so a wrong argument count
//! passed `hyb check` with zero diagnostics and failed `go build` verbatim
//! (`too many arguments in call to id[int]` / `... to id`; Row 13
//! BUILDABLE: reject fail-closed, HYB-E1002). The fix adds
//! `check_generic_call_arity`: the argument count of an explicit
//! instantiation (`id[int](10, 20)`) and an inferred call (`id(10, 20)`)
//! is checked, plus the type-argument count of an explicit instantiation
//! (Go refuses `got 2 type arguments but want 1`). The type arguments
//! themselves stay uninferrable: a wrong argument *type* at a correct
//! count still fails open, and a lone multi-value call stays with
//! #682 U7. No valid call may be rejected (never over-reject; Go accepts
//! all of the accept cases below).

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects_e1002(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {:?}, got {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "verdict for {:?}: {:?}", src, r.diagnostics);
}

#[test]
fn generic_explicit_instantiation_rejects_wrong_arg_count() {
    rejects_e1002("fn id[t any](x t) t {\n    return x\n}\n\nfn main() {\n    v := id[int](10, 20, 30)\n    _ = v\n}\n");
    rejects_e1002("fn id[t any](x t) t {\n    return x\n}\n\nfn main() {\n    v := id[int](10, 20)\n    _ = v\n}\n");
    rejects_e1002("fn id[t any](x t) t {\n    return x\n}\n\nfn main() {\n    v := id[int]()\n    _ = v\n}\n");
}

#[test]
fn generic_inferred_call_rejects_wrong_arg_count() {
    rejects_e1002("fn id[t any](x t) t {\n    return x\n}\n\nfn main() {\n    v := id(10, 20, 30)\n    _ = v\n}\n");
    rejects_e1002("fn id[t any](x t) t {\n    return x\n}\n\nfn main() {\n    v := id(10, 20)\n    _ = v\n}\n");
    rejects_e1002("fn id[t any](x t) t {\n    return x\n}\n\nfn main() {\n    v := id()\n    _ = v\n}\n");
}

#[test]
fn generic_explicit_instantiation_rejects_wrong_type_arg_count() {
    // Go refuses `got 2 type arguments but want 1` before it looks at the
    // arguments; the decl-level pre-pass already fired this, the call-site
    // branch is defense in depth.
    rejects_e1002("fn id[t any](x t) t {\n    return x\n}\n\nfn main() {\n    v := id[int, string](10)\n    _ = v\n}\n");
}

#[test]
fn generic_calls_with_exact_arity_accept() {
    accepts("fn id[t any](x t) t {\n    return x\n}\n\nfn main() {\n    v := id[int](10)\n    _ = v\n}\n");
    accepts("fn id[t any](x t) t {\n    return x\n}\n\nfn main() {\n    v := id(10)\n    _ = v\n}\n");
    accepts("fn pair[t any, u any](a t, b u) (t, u) {\n    return a, b\n}\n\nfn main() {\n    x, y := pair[int, string](10, \"hi\")\n    _, _ = x, y\n}\n");
    accepts("fn pair[t any, u any](a t, b u) (t, u) {\n    return a, b\n}\n\nfn main() {\n    x, y := pair(10, \"hi\")\n    _, _ = x, y\n}\n");
    // A second type parameter the body never mentions stays uninferrable,
    // but a correct explicit count must not be rejected either.
    accepts("fn id[t any, u any](x t) t {\n    return x\n}\n\nfn main() {\n    v := id[int, string](10)\n    _ = v\n}\n");
}
