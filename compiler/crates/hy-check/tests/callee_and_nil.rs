//! callee_and_nil.rs — regression tests for the O4.0 spawn-callee proof and
//! the N1 nil rule (HYB-E4001), from the external adoption-audit findings
//! F1 (task callee never classified — closure and undeclared callees both
//! silently accepted) and F2 (E4001 existed only as registry text — no
//! emission site; `nil` flowed into maps/slices everywhere).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

// ---------------------------------------------------------------------------
// O4.0 — the spawn-callee proof
// ---------------------------------------------------------------------------

#[test]
fn closure_callee_is_rejected_e3001() {
    // A closure captures its environment: spawning it shares every capture
    // (here a live map) with the new task — the A5 perimeter.
    let r = check(
        "fn main() {\n    counts := map[string]int{}\n    g := fn() { counts[\"k\"] = 1 }\n    task g()\n}\n",
    );
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001 for the closure callee, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn closure_literal_callee_is_rejected_e3001() {
    let r = check(
        "fn main() {\n    counts := map[string]int{}\n    task (fn() { counts[\"k\"] = 1 })()\n}\n",
    );
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001 for the closure-literal callee, got {:?}",
        cs
    );
}

#[test]
fn undeclared_callee_is_fail_closed_e3005() {
    let r = check("fn main() {\n    task mystery_fn(1)\n}\n");
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3005"),
        "expected HYB-E3005 for the undeclared callee, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn local_non_fn_callee_is_fail_closed_e3005() {
    // A resolved local binding that is not a declared fn cannot prove what
    // would run — fail-closed (the int here is also just not callable).
    let r = check("fn main() {\n    x := 5\n    task x()\n}\n");
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3005"),
        "expected HYB-E3005 for the non-fn local callee, got {:?}",
        cs
    );
}

#[test]
fn declared_toplevel_fn_callee_is_proven() {
    // The control: a static call to a declared top-level fn crosses nothing
    // but its (primitive) argument — accept with zero errors.
    let r = check("fn work(x int) int { return x + 1 }\n\nfn main() {\n    task work(1)\n}\n");
    assert!(
        codes(&r).is_empty(),
        "expected zero errors, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn local_binding_shadows_fn_table_for_the_callee_proof() {
    // `work` is declared top-level AND shadowed by a closure binding: the
    // LOCAL value is what would run — the proof must reject.
    let r = check(
        "fn work(x int) int { return x + 1 }\n\nfn main() {\n    counts := map[string]int{}\n    work := fn() { counts[\"k\"] = 1 }\n    task work()\n}\n",
    );
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001 — the local closure shadows the declared fn, got {:?}",
        cs
    );
}

#[test]
fn closure_callee_inside_managed_takes_the_hatch() {
    // The hatch covers sendability doubts: a deliberate actor-loop closure
    // is the documented managed("...", "actor") shape — warning, no error.
    let r = check(
        "fn main() {\n    counts := map[string]int{}\n    managed(\"single-owner loop\", \"actor\") {\n        g := fn() { counts[\"k\"] = 1 }\n        task g()\n    }\n}\n",
    );
    assert!(
        codes(&r).is_empty(),
        "managed suppresses the closure-callee reject; got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r
        .diagnostics
        .iter()
        .any(|d| d.code == "HYB-E8001" && d.severity == Severity::Warning));
}

#[test]
fn explain_e8001_example_warns_without_reject() {
    let r = check(
        "fn tally(counter map[string]int) {}\n\
         fn main() {\n\
             counter := map[string]int{}\n\
             managed(\"counter access guarded by a mutex\", \"mutex\") { task tally(counter) }\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(
        codes(&r).is_empty(),
        "expected zero errors for E8001 example; got {:?}",
        codes(&r)
    );
    assert!(r
        .diagnostics
        .iter()
        .any(|d| d.code == "HYB-E8001" && d.severity == Severity::Warning));
}

// ---------------------------------------------------------------------------
// N1 — nil has exactly two homes (error, foreign); everywhere else E4001
// ---------------------------------------------------------------------------

#[test]
fn nil_assign_to_map_is_e4001() {
    let r = check("fn main() {\n    var m map[string]int = map[string]int{}\n    m = nil\n}\n");
    assert!(
        codes(&r).iter().any(|c| c == "HYB-E4001"),
        "expected HYB-E4001 for `m = nil`, got {:?}",
        codes(&r)
    );
}

#[test]
fn nil_init_to_ascribed_slice_is_e4001() {
    // The SPEC §3.1's own example: `var s []int = nil`.
    let r = check("fn main() {\n    var s []int = nil\n}\n");
    assert!(
        codes(&r).iter().any(|c| c == "HYB-E4001"),
        "expected HYB-E4001 for `var s []int = nil`, got {:?}",
        codes(&r)
    );
}

#[test]
fn nil_send_into_map_chan_is_e4001() {
    let r = check("fn main() {\n    ch := make(chan map[string]int, 1)\n    ch <- nil\n}\n");
    assert!(
        codes(&r).iter().any(|c| c == "HYB-E4001"),
        "expected HYB-E4001 for `ch <- nil`, got {:?}",
        codes(&r)
    );
}

#[test]
fn nil_spawn_arg_to_map_param_is_e4001() {
    let r = check("fn sink(m map[string]int) int { return m[\"k\"] }\n\nfn main() {\n    task sink(nil)\n}\n");
    assert!(
        codes(&r).iter().any(|c| c == "HYB-E4001"),
        "expected HYB-E4001 for `task sink(nil)`, got {:?}",
        codes(&r)
    );
}

#[test]
fn nil_assign_to_primitive_is_e4001() {
    let r = check("fn main() {\n    x := 5\n    x = nil\n}\n");
    assert!(
        codes(&r).iter().any(|c| c == "HYB-E4001"),
        "expected HYB-E4001 for `x = nil` on a primitive, got {:?}",
        codes(&r)
    );
}

#[test]
fn nil_return_against_map_result_is_e4001() {
    let r = check("fn load() map[string]int {\n    return nil\n}\n\nfn main() {\n    _ = load()\n}\n");
    assert!(
        codes(&r).iter().any(|c| c == "HYB-E4001"),
        "expected HYB-E4001 for `return nil` under a map result, got {:?}",
        codes(&r)
    );
}

#[test]
fn nil_is_e4001_even_inside_managed() {
    // The hatch covers sendability doubts, NOT the null-safety contract:
    // a type error stays a type error.
    let r = check(
        "fn sink(m map[string]int) int { return m[\"k\"] }\n\nfn main() {\n    managed(\"interop edge\", \"interop\") {\n        task sink(nil)\n    }\n}\n",
    );
    assert!(
        codes(&r).iter().any(|c| c == "HYB-E4001"),
        "E4001 must fire inside managed; got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn error_nil_stays_legal() {
    // N1's scoped home #1: `nil` is the zero value of `error`.
    let r = check(
        "fn might_fail() (int, error) {\n    return 0, nil\n}\n\nfn main() {\n    x, err := might_fail()\n    _ = x\n    _ = err\n}\n",
    );
    assert!(
        codes(&r).is_empty(),
        "error-nil must stay legal, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn nil_to_unknown_target_is_fail_closed_at_the_boundary_not_e4001() {
    // The target type did not resolve: N1 cannot fire, but the boundary is
    // fail-closed (O4.9) — the nil is UNPROVEN, never silently ok.
    let r = check("fn main() {\n    ch := make(chan mystery_type, 1)\n    ch <- nil\n}\n");
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3005"),
        "expected fail-closed E3005 for a nil whose target did not resolve, got {:?}",
        cs
    );
}

#[test]
fn zero_values_stay_legal() {
    // The N3 zero-value routes the fix-its teach.
    let r = check(
        "fn main() {\n    var m map[string]int = map[string]int{}\n    var s []int = []int{}\n    var n int = 0\n    m = map[string]int{}\n    s = []int{}\n    _ = m\n    _ = s\n    _ = n\n}\n",
    );
    assert!(
        codes(&r).is_empty(),
        "zero-value forms must stay legal, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn nil_map_key_is_e4001() {
    // Map composite literal with nil in non-nilable key slot -> HYB-E4001 (Row 13 / N1)
    let r = check("fn main() {\n    _ = map[int]string{nil: \"v\"}\n}\n");
    assert!(
        codes(&r).iter().any(|c| c == "HYB-E4001"),
        "expected HYB-E4001 for `nil` map key, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn nil_map_value_is_e4001() {
    // Map composite literal with nil in non-nilable value slot -> HYB-E4001 (Row 13 / N1)
    let r = check("fn main() {\n    _ = map[string]int{\"k\": nil}\n}\n");
    assert!(
        codes(&r).iter().any(|c| c == "HYB-E4001"),
        "expected HYB-E4001 for `nil` map value, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

