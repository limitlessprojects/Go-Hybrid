//! assignment_mismatch_and_parallel_types.rs — tests for assignment count and type checking
//! (HYB-E1002 / BUILDABLE).
//!
//! Validates:
//! 1. Count mismatch in local definitions: `a, b, c := 1, 2` or `a, b := 1, 2, 3` or `a, b := 1`.
//! 2. Count mismatch in assignments: `a, b, c = 1, 2` or `a, b = 1, 2, 3` or `a, b = 1`.
//! 3. Comma-ok arity mismatch: `a, b, c := m[k]` or `a, b, c := <-ch`.
//! 4. Single-result call assigned to multiple variables: `a, b := one()`.
//! 5. Parallel assignment type conflicts: `a, b = "hello", 123` where `a: int, b: string`.
//! 6. Local declaration type conflicts: `var a int = "hello"` or `var a int = 1, 2`.
//! 7. Legal assignments, comma-ok, and multi-value bindings stay cleanly accepted.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_rejects_e1002(r: &hy_check::CheckResult, what: &str) {
    let codes = error_codes(r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {}, got {:?}",
        what,
        codes
    );
    assert_eq!(r.verdict, Verdict::Reject, "{}", what);
}

fn assert_ok(src: &str, what: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected clean accept for {}, got diagnostics: {:?}",
        what,
        error_codes(&r)
    );
}

#[test]
fn define_count_mismatch_rejected() {
    let src1 = "fn main() {\n    a, b, c := 1, 2\n    fmt.println(\"{a} {b} {c}\")\n}\n";
    assert_rejects_e1002(&check(src1), "a, b, c := 1, 2 (3 names, 2 values)");

    let src2 = "fn main() {\n    a, b := 1, 2, 3\n    fmt.println(\"{a} {b}\")\n}\n";
    assert_rejects_e1002(&check(src2), "a, b := 1, 2, 3 (2 names, 3 values)");

    let src3 = "fn main() {\n    a, b := 1\n    fmt.println(\"{a} {b}\")\n}\n";
    assert_rejects_e1002(&check(src3), "a, b := 1 (2 names, 1 value)");
}

#[test]
fn assign_count_mismatch_rejected() {
    let src1 = "fn main() {\n    var a int\n    var b int\n    var c int\n    a, b, c = 1, 2\n    fmt.println(\"{a} {b} {c}\")\n}\n";
    assert_rejects_e1002(&check(src1), "a, b, c = 1, 2 (3 targets, 2 values)");

    let src2 = "fn main() {\n    var a int\n    var b int\n    a, b = 1, 2, 3\n    fmt.println(\"{a} {b}\")\n}\n";
    assert_rejects_e1002(&check(src2), "a, b = 1, 2, 3 (2 targets, 3 values)");

    let src3 = "fn main() {\n    var a int\n    var b int\n    a, b = 1\n    fmt.println(\"{a} {b}\")\n}\n";
    assert_rejects_e1002(&check(src3), "a, b = 1 (2 targets, 1 value)");
}

#[test]
fn single_result_call_into_multiple_targets_rejected() {
    let src1 = "fn one() int {\n    return 1\n}\nfn main() {\n    a, b := one()\n    fmt.println(\"{a} {b}\")\n}\n";
    assert_rejects_e1002(&check(src1), "a, b := one()");

    let src2 = "fn one() int {\n    return 1\n}\nfn main() {\n    var a int\n    var b int\n    a, b = one()\n    fmt.println(\"{a} {b}\")\n}\n";
    assert_rejects_e1002(&check(src2), "a, b = one()");
}

#[test]
fn parallel_assign_type_mismatch_rejected() {
    let src1 = "fn main() {\n    var a int\n    var b string\n    a, b = \"hello\", 123\n    fmt.println(\"{a} {b}\")\n}\n";
    assert_rejects_e1002(&check(src1), "a, b = \"hello\", 123");

    let src2 = "fn main() {\n    var a int\n    var b int\n    a, b = 10, \"mismatch\"\n    fmt.println(\"{a} {b}\")\n}\n";
    assert_rejects_e1002(&check(src2), "a, b = 10, \"mismatch\"");
}

#[test]
fn var_decl_type_and_count_mismatch_rejected() {
    let src1 = "fn main() {\n    var a int = \"hello\"\n    fmt.println(\"{a}\")\n}\n";
    assert_rejects_e1002(&check(src1), "var a int = \"hello\"");

    let src2 = "fn main() {\n    var a int = 1, 2\n    fmt.println(\"{a}\")\n}\n";
    assert_rejects_e1002(&check(src2), "var a int = 1, 2");
}

#[test]
fn comma_ok_arity_mismatch_rejected() {
    let src1 = "fn main() {\n    m := map[string]int{}\n    a, b, c := m[\"k\"]\n    fmt.println(\"{a} {b} {c}\")\n}\n";
    assert_rejects_e1002(&check(src1), "a, b, c := m[\"k\"]");

    let src2 = "fn main() {\n    c := make(chan int, 1)\n    a, b, c_val := <-c\n    fmt.println(\"{a} {b} {c_val}\")\n}\n";
    assert_rejects_e1002(&check(src2), "a, b, c_val := <-c");
}

#[test]
fn legal_assignments_stay_accepted() {
    let src = "fn two() (int, int) {\n    return 1, 2\n}\nfn three() (int, string, bool) {\n    return 1, \"hi\", true\n}\nfn main() {\n    a, b := 1, 2\n    fmt.println(\"{a} {b}\")\n    var x int = 10\n    var y int\n    x, y = 3, 4\n    fmt.println(\"{x} {y}\")\n    p, q := two()\n    fmt.println(\"{p} {q}\")\n    m := map[string]int{}\n    m[\"k\"] = 42\n    v, ok := m[\"k\"]\n    fmt.println(\"{v} {ok}\")\n    ch := make(chan int, 1)\n    ch <- 99\n    val, recv_ok := <-ch\n    fmt.println(\"{val} {recv_ok}\")\n    n1, n2, n3 := three()\n    fmt.println(\"{n1} {n2} {n3}\")\n}\n";
    assert_ok(src, "legal assignments, definitions, and comma-ok");
}
