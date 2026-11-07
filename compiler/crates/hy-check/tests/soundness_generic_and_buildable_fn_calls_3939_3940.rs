//! Regression tests for #3939 (interprocedural may_share and const-mutation on
//! explicit generic calls `f[T](...)` and associated function calls `Type.fn(...)`)
//! and #3940 (argument-type and callable validation on local closures, fn-typed
//! bindings, fn-typed struct fields, and `task` spawns).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
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
        "fn id[T](x T) T {\n\
         \x20   return x\n\
         }\n\
         fn worker(m map[string]int) {\n\
         \x20   m[\"a\"] = 1\n\
         }\n\
         fn main() {\n\
         \x20   m := map[string]int{\"a\": 0}\n\
         \x20   alias := id[map[string]int](m)\n\
         \x20   task worker(alias)\n\
         \x20   m[\"b\"] = 2\n\
         }\n",
    );
    assert_code(&r, "HYB-E3001", "explicit generic call may_share propagation");
}

#[test]
fn explicit_generic_and_assoc_calls_enforce_const_mutation_e3006() {
    let r1 = check(
        "const K = map[string]int{\"a\": 1}\n\
         fn mutate[T](m map[string]int, v T) {\n\
         \x20   m[\"a\"] = 2\n\
         }\n\
         fn main() {\n\
         \x20   mutate[int](K, 0)\n\
         }\n",
    );
    assert_code(&r1, "HYB-E3006", "explicit generic call const mutation");

    let r2 = check(
        "const K = map[string]int{\"a\": 1}\n\
         struct S {}\n\
         fn (S) touch(m map[string]int) {\n\
         \x20   m[\"a\"] = 2\n\
         }\n\
         fn main() {\n\
         \x20   S.touch(K)\n\
         }\n",
    );
    assert_code(&r2, "HYB-E3006", "associated fn call const mutation");
}

#[test]
fn local_closure_and_fn_field_arg_mismatch_rejected_e1002() {
    let r1 = check(
        "fn main() {\n\
         \x20   f := fn(x int) int { return x + 1 }\n\
         \x20   _ = f(\"wrong\")\n\
         }\n",
    );
    assert_code(&r1, "HYB-E1002", "local closure call arg type mismatch");

    let r2 = check(
        "struct Runner {\n\
         \x20   cb fn(int) int\n\
         }\n\
         fn run(r Runner) int {\n\
         \x20   return r.cb(\"wrong\")\n\
         }\n",
    );
    assert_code(&r2, "HYB-E1002", "struct fn field call arg type mismatch");
}

#[test]
fn task_spawn_on_non_callable_or_wrong_arg_types_rejected_e1002() {
    let r1 = check(
        "fn main() {\n\
         \x20   n := 42\n\
         \x20   task n()\n\
         }\n",
    );
    assert_code(&r1, "HYB-E1002", "task spawn on non-callable local int");

    let r2 = check(
        "fn main() {\n\
         \x20   f := fn(x int) {}\n\
         \x20   task f(\"wrong\")\n\
         }\n",
    );
    assert_code(&r2, "HYB-E1002", "task spawn on local closure with mismatched arg type");
}
