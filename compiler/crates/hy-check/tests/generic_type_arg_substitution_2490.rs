//! generic_type_arg_substitution_2490.rs — regression for issue #2490: a generic
//! function's result type is written over its type parameters (`[1]t`, `[]t`,
//! `map[k]v`, `*t`), and resolving it without binding those parameters collapsed
//! the shape to container-of-`Unknown`. Because `shape_may_share`,
//! `shape_holds_closure` and `shape_holds_unmovable_leaf` all answer `false` for
//! an inner `Unknown` leaf, three Tier-0 rules failed open with 0 diagnostics:
//!
//! 1. O5.2/O7 alias-freeze — `a := wrap_arr(m)` did not record the may-share edge,
//!    so `task worker(move a, done)` succeeded while the parent kept mutating `m`
//!    (live `fatal error: concurrent map writes`);
//! 2. A5 — `cbs := wrap_slice(fn() { count += 1 })` did not read as a container of
//!    closures, so it could be smuggled into another goroutine (data race);
//! 3. O4.9 — moving a container whose declared type still mentions an unbound type
//!    parameter (`fn relay[t](xs []t, ch chan []t) { ch <- move xs }`) bypassed the
//!    fail-closed boundary rule.
//!
//! The fix binds the parameters per call site (explicit `f[T](...)` folded into the
//! callee name, and inferred from the arguments) and records the function's own
//! type parameters so an uninstantiated container fails closed. The controls below
//! pin that the same generic does NOT start rejecting when no boundary is involved,
//! when the argument is a fresh literal, or when the site is `managed`.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
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

fn rejects_with(src: &str, code: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == code),
        "expected {} for {:?}, got {:?}",
        code,
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

/// The boundary codes this fix is about — a control may carry other diagnostics
/// (e.g. the managed lint) but must not be rejected by any of these.
fn boundary_codes(r: &CheckResult) -> Vec<String> {
    ["HYB-E3001", "HYB-E3002", "HYB-E3003", "HYB-E3005"]
        .iter()
        .filter(|c| error_codes(r).iter().any(|d| d == *c))
        .map(|c| c.to_string())
        .collect()
}

fn no_boundary_reject(src: &str) {
    let r = check(src);
    let cs = boundary_codes(&r);
    assert!(
        cs.is_empty(),
        "must not be rejected at the boundary for {:?}, got {:?}",
        src,
        cs
    );
}

const REPRO_ARRAY: &str = r#"
fn wrap_arr[t](x t) [1]t {
    return [1]t{x}
}

fn worker(arr [1]map[int]int, done chan bool) {
    for i in 0..50000 {
        arr[0][i] = i
    }
    done <- true
}

fn main() {
    m := map[int]int{0: 0}
    a := wrap_arr(m)
    done := make(chan bool, 1)
    task worker(move a, done)
    for i in 0..50000 {
        m[i] = i + 1
    }
    <-done
}
"#;

const REPRO_SLICE: &str = r#"
fn wrap_slice[t](x t) []t {
    return []t{x}
}

fn main() {
    count := 0
    cbs := wrap_slice(fn() {
        count += 1
    })
    ch := make(chan []fn(), 1)
    ch <- move cbs
    for i in 0..20000 {
        count += 1
    }
}
"#;

const EXPLICIT_TYPE_ARG: &str = r#"
fn wrap_arr[t](x t) [1]t {
    return [1]t{x}
}

fn worker(arr [1]map[int]int, done chan bool) {
    arr[0][0] = 1
    done <- true
}

fn main() {
    m := map[int]int{0: 0}
    a := wrap_arr[map[int]int](m)
    done := make(chan bool, 1)
    task worker(move a, done)
    m[1] = 1
    <-done
}
"#;

const UNBOUND_RELAY: &str = r#"
fn relay[t](xs []t, ch chan []t) {
    ch <- move xs
}

fn main() {
    count := 0
    ch := make(chan []fn(), 1)
    relay([]fn(){ fn() { count += 1 } }, ch)
    count += 1
}
"#;

#[test]
fn generic_array_return_freezes_the_aliased_argument() {
    rejects_with(REPRO_ARRAY, "HYB-E3003");
}

#[test]
fn generic_slice_return_holding_a_closure_is_rejected() {
    rejects_with(REPRO_SLICE, "HYB-E3002");
}

#[test]
fn explicit_type_argument_binds_like_the_inferred_one() {
    rejects_with(EXPLICIT_TYPE_ARG, "HYB-E3003");
}

#[test]
fn moving_an_uninstantiated_generic_container_fails_closed() {
    rejects_with(UNBOUND_RELAY, "HYB-E3005");
}

#[test]
fn the_same_generic_without_a_boundary_stays_accepted() {
    no_boundary_reject(
        "fn wrap_arr[t](x t) [1]t {\n    return [1]t{x}\n}\n\nfn main() {\n    m := map[int]int{0: 0}\n    a := wrap_arr(m)\n    a[0][1] = 1\n    m[2] = 2\n}\n",
    );
}

#[test]
fn a_fresh_argument_at_a_boundary_stays_accepted() {
    no_boundary_reject(
        "fn wrap_arr[t](x t) [1]t {\n    return [1]t{x}\n}\n\nfn worker(arr [1]map[int]int, done chan bool) {\n    arr[0][0] = 1\n    done <- true\n}\n\nfn main() {\n    a := wrap_arr(map[int]int{0: 0})\n    done := make(chan bool, 1)\n    task worker(move a, done)\n    <-done\n}\n",
    );
}

#[test]
fn a_concrete_wrapper_control_stays_accepted() {
    no_boundary_reject(
        "fn wrap_concrete(x map[int]int) [1]map[int]int {\n    return [1]map[int]int{x}\n}\n\nfn worker(arr [1]map[int]int, done chan bool) {\n    arr[0][0] = 1\n    done <- true\n}\n\nfn main() {\n    a := wrap_concrete(map[int]int{0: 0})\n    done := make(chan bool, 1)\n    task worker(move a, done)\n    <-done\n}\n",
    );
}

#[test]
fn a_two_parameter_generic_binds_both_explicit_arguments() {
    // `k` and `v` both bound from `wrap3[int, int](m)`: the returned `[1]map[k]v`
    // must become `Array(Map(Int, Int))`, so the alias `m` is inside the freeze
    // component of the moved `a` (O5.2).
    rejects_with(
        "fn wrap3[k comparable, v](m map[k]v) [1]map[k]v {\n    return [1]map[k]v{m}\n}\n\nfn worker(arr [1]map[int]int, done chan bool) {\n    arr[0][0] = 1\n    done <- true\n}\n\nfn main() {\n    m := map[int]int{0: 0}\n    a := wrap3[int, int](m)\n    done := make(chan bool, 1)\n    task worker(move a, done)\n    m[1] = 1\n    <-done\n}\n",
        "HYB-E3003",
    );
}

#[test]
fn a_generic_identity_used_locally_stays_accepted() {
    no_boundary_reject(
        "fn id[t](x t) t {\n    return x\n}\n\nfn main() {\n    m := map[int]int{0: 0}\n    n := id(m)\n    n[1] = 1\n    m[2] = 2\n}\n",
    );
}

#[test]
fn a_managed_site_keeps_only_the_managed_lint() {
    // `check_file` (the library entry point) runs the Tier-0 rules; the managed
    // lint (`HYB-E8001`) is added by the CLI's `--report-managed` path, so this
    // control asserts the boundary rule is suppressed here and leaves the lint
    // to the CLI-level tests.
    let r = check(
        "fn wrap_slice[t](x t) []t {\n    return []t{x}\n}\n\nfn main() {\n    count := 0\n    cbs := wrap_slice(fn() { count += 1 })\n    ch := make(chan []fn(), 1)\n    managed(\"the channel is drained before this task mutates, reviewed\", \"actor\") {\n        ch <- move cbs\n    }\n}\n",
    );
    assert!(
        boundary_codes(&r).is_empty(),
        "the hatch suppresses the boundary rule: {:?}",
        boundary_codes(&r)
    );
}

