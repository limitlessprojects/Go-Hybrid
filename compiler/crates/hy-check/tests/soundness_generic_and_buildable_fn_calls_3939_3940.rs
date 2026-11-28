//! Regression tests for #3939 (interprocedural may_share and const-mutation on
//! explicit generic calls `f[t](...)` and associated function calls `s.fn(...)`)
//! and #3940 (argument-type and callable validation on local closures, fn-typed
//! bindings, fn-typed struct fields, and `task` spawns).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
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

fn assert_code(r: &hy_check::CheckResult, expected: &str, what: &str) {
    let cs = codes(r);
    assert!(
        cs.iter().any(|c| c == expected),
        "expected {} ({}), got {:?}",
        expected,
        what,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn explicit_generic_call_propagates_may_share_to_task_e3001() {
    let r = check(
        r#"
fn id[t any](x t) t {
    return x
}

fn worker(m map[string]int) {
    m["a"] = 1
}

fn main() {
    m := map[string]int{"a": 0}
    alias := id[map[string]int](m)
    task worker(alias)
    m["b"] = 2
}
"#,
    );
    assert_code(&r, "HYB-E3001", "explicit generic call may_share propagation");
}

#[test]
fn explicit_generic_and_assoc_calls_enforce_const_mutation_e3006() {
    let r1 = check(
        r#"
const k = map[string]int{"a": 1}

fn mutate[t any](m map[string]int, v t) {
    m["a"] = 2
}

fn main() {
    mutate[int](k, 0)
}
"#,
    );
    assert_code(&r1, "HYB-E3006", "explicit generic call const mutation");

    let r2 = check(
        r#"
const k = map[string]int{"a": 1}

struct s {}

fn s.touch(m map[string]int) {
    m["a"] = 2
}

fn main() {
    s.touch(k)
}
"#,
    );
    assert_code(&r2, "HYB-E3006", "associated fn call const mutation");
}

#[test]
fn local_closure_and_fn_field_arg_mismatch_rejected_e1002() {
    let r1 = check(
        r#"
fn main() {
    f := fn(x int) int { return x + 1 }
    _ = f("wrong")
}
"#,
    );
    assert_code(&r1, "HYB-E1002", "local closure call arg type mismatch");

    let r2 = check(
        r#"
struct runner {
    cb fn(int) int
}

fn run(r runner) int {
    return r.cb("wrong")
}
"#,
    );
    assert_code(&r2, "HYB-E1002", "struct fn field call arg type mismatch");
}

#[test]
fn task_spawn_on_non_callable_or_wrong_arg_types_rejected_e1002() {
    let r1 = check(
        r#"
fn main() {
    n := 42
    task n()
}
"#,
    );
    assert_code(&r1, "HYB-E1002", "task spawn on non-callable local int");

    let r2 = check(
        r#"
fn main() {
    f := fn(x int) {}
    task f("wrong")
}
"#,
    );
    assert_code(&r2, "HYB-E1002", "task spawn on local closure with mismatched arg type");
}
