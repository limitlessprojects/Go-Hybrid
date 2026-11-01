//! Regression tests for #1174:
//! Multi-value call forwarding directly into boundary arguments (`task worker(pair(m))`,
//! `task worker((pair_try(m))?)`, methods, generic methods, fn-typed fields/slices, and
//! local closures) must sendability-classify ALL forwarded return values (`HYB-E3001` /
//! `HYB-E3005`), while accepting all-sendable results and auditing `managed(...)` (`HYB-E8001`).

use hy_check::{check_file, CheckResult, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

#[test]
fn spawn_multi_value_call_rejects_non_sendable_second_result() {
    let r = check(
        "fn pair(m map[string]int) (int, map[string]int) {\n\
           return 1, m\n\
         }\n\
         fn worker(n int, m map[string]int) {\n\
           m[\"worker\"] = n\n\
         }\n\
         fn main() {\n\
           m := map[string]int{\"init\": 0}\n\
           task worker(pair(m))\n\
           m[\"main\"] = 2\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "diags: {:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "expected HYB-E3001 on multi-value call forwarding non-sendable map, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn spawn_multi_value_try_call_rejects_non_sendable_tail_result() {
    let r = check(
        "fn pair_try(m map[string]int) (int, map[string]int, error) {\n\
           return 1, m, nil\n\
         }\n\
         fn worker(n int, m map[string]int) {\n\
           m[\"worker\"] = n\n\
         }\n\
         fn run() error {\n\
           m := map[string]int{\"init\": 0}\n\
           task worker((pair_try(m))?)\n\
           m[\"main\"] = 2\n\
           return nil\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "diags: {:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "expected HYB-E3001 on try-unwrapped multi-value call forwarding non-sendable map, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn spawn_multi_value_method_and_generic_method_reject_non_sendable_result() {
    let r_method = check(
        "struct box {\n\
           tag int\n\
         }\n\
         fn (b box) pair(s []int) (int, []int) {\n\
           return b.tag, s\n\
         }\n\
         fn worker(n int, s []int) {\n\
           s[0] = n\n\
         }\n\
         fn main() {\n\
           b := box{tag: 1}\n\
           s := []int{0}\n\
           task worker(b.pair(s))\n\
           s[0] = 2\n\
         }\n",
    );
    assert_eq!(
        r_method.verdict,
        Verdict::Reject,
        "diags: {:?}",
        r_method.diagnostics
    );
    assert!(
        r_method.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "expected HYB-E3001 on multi-value method call, got: {:?}",
        r_method.diagnostics
    );

    let r_gen = check(
        "struct gbox[t] {\n\
           item t\n\
         }\n\
         fn (g gbox[t]) pair() (int, t) {\n\
           return 1, g.item\n\
         }\n\
         fn worker(n int, s []int) {\n\
           s[0] = n\n\
         }\n\
         fn main() {\n\
           s := []int{0}\n\
           g := gbox[[]int]{item: s}\n\
           task worker(g.pair())\n\
           s[0] = 2\n\
         }\n",
    );
    assert_eq!(r_gen.verdict, Verdict::Reject, "diags: {:?}", r_gen.diagnostics);
    assert!(
        r_gen.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "expected HYB-E3001 on generic multi-value method call, got: {:?}",
        r_gen.diagnostics
    );
}

#[test]
fn spawn_multi_value_fn_field_indexed_and_closure_callees_reject_non_sendable_result() {
    let r_field = check(
        "struct holder {\n\
           f fn(map[string]int) int map[string]int\n\
         }\n\
         fn pair(m map[string]int) (int, map[string]int) {\n\
           return 1, m\n\
         }\n\
         fn worker(n int, m map[string]int) {\n\
           m[\"worker\"] = n\n\
         }\n\
         fn main() {\n\
           m := map[string]int{\"init\": 0}\n\
           h := holder{f: pair}\n\
           task worker(h.f(m))\n\
           m[\"main\"] = 2\n\
         }\n",
    );
    assert_eq!(
        r_field.verdict,
        Verdict::Reject,
        "diags: {:?}",
        r_field.diagnostics
    );
    assert!(
        r_field.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "expected HYB-E3001 on fn-typed struct field multi-value call, got: {:?}",
        r_field.diagnostics
    );

    let r_index = check(
        "fn pair(m map[string]int) (int, map[string]int) {\n\
           return 1, m\n\
         }\n\
         fn worker(n int, m map[string]int) {\n\
           m[\"worker\"] = n\n\
         }\n\
         fn main() {\n\
           m := map[string]int{\"init\": 0}\n\
           fs := []fn(map[string]int) int map[string]int{pair}\n\
           task worker(fs[0](m))\n\
           m[\"main\"] = 2\n\
         }\n",
    );
    assert_eq!(
        r_index.verdict,
        Verdict::Reject,
        "diags: {:?}",
        r_index.diagnostics
    );
    assert!(
        r_index.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "expected HYB-E3001 on indexed fn multi-value call, got: {:?}",
        r_index.diagnostics
    );

    let r_local_fn = check(
        "fn worker(n int, m map[string]int) {\n\
           m[\"worker\"] = n\n\
         }\n\
         fn main() {\n\
           m := map[string]int{\"init\": 0}\n\
           local_pair := fn(x map[string]int) (int, map[string]int) {\n\
             return 1, x\n\
           }\n\
           task worker(local_pair(m))\n\
           m[\"main\"] = 2\n\
         }\n",
    );
    assert_eq!(
        r_local_fn.verdict,
        Verdict::Reject,
        "diags: {:?}",
        r_local_fn.diagnostics
    );
    assert!(
        r_local_fn.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "expected HYB-E3001 on local closure multi-value call, got: {:?}",
        r_local_fn.diagnostics
    );

    let r_in_closure = check(
        "fn pair(m map[string]int) (int, map[string]int) {\n\
           return 1, m\n\
         }\n\
         fn worker(n int, m map[string]int) {\n\
           m[\"worker\"] = n\n\
         }\n\
         fn main() {\n\
           m := map[string]int{\"init\": 0}\n\
           run := fn() {\n\
             task worker(pair(m))\n\
           }\n\
           run()\n\
           m[\"main\"] = 2\n\
         }\n",
    );
    assert_eq!(
        r_in_closure.verdict,
        Verdict::Reject,
        "diags: {:?}",
        r_in_closure.diagnostics
    );
    assert!(
        r_in_closure.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "expected HYB-E3001 inside closure body, got: {:?}",
        r_in_closure.diagnostics
    );
}

#[test]
fn spawn_multi_value_call_fails_closed_on_unproven_second_result() {
    let r = check(
        "fn pair_any(n int) (int, any) {\n\
           return n, n\n\
         }\n\
         fn worker(n int, v any) {\n\
         }\n\
         fn main() {\n\
           task worker(pair_any(1))\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "diags: {:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3005"),
        "expected HYB-E3005 fail-closed diagnostic on unproven second return value, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn spawn_multi_value_call_accepts_all_sendable_results_and_audits_managed() {
    let r_ok = check(
        "fn pair_safe() (int, string, error) {\n\
           return 7, \"ok\", nil\n\
         }\n\
         fn worker(n int, s string) {\n\
         }\n\
         fn run() error {\n\
           task worker(pair_safe()?)\n\
           return nil\n\
         }\n",
    );
    assert_eq!(r_ok.verdict, Verdict::Accept, "diags: {:?}", r_ok.diagnostics);
    assert!(
        r_ok.diagnostics.is_empty(),
        "expected 0 diagnostics when all forwarded results are sendable, got: {:?}",
        r_ok.diagnostics
    );

    let r_managed = check(
        "fn pair(m map[string]int) (int, map[string]int) {\n\
           return 1, m\n\
         }\n\
         fn worker(n int, m map[string]int) {\n\
         }\n\
         fn main() {\n\
           m := map[string]int{\"init\": 0}\n\
           managed(\"read-only after barrier\", \"other\") {\n\
             task worker(pair(m))\n\
           }\n\
         }\n",
    );
    assert_eq!(
        r_managed.verdict,
        Verdict::Accept,
        "diags: {:?}",
        r_managed.diagnostics
    );
    assert!(
        r_managed.diagnostics.iter().any(|d| d.code == "HYB-E8001"),
        "expected HYB-E8001 managed audit warning, got: {:?}",
        r_managed.diagnostics
    );
}
