//! task_spawn_and_parallel_array_3088.rs — issue #3088 (Row 13 BUILDABLE).
//!
//! 1. `Stmt::Task` must validate its callee and arguments with the same
//!    operand, arity, and non-function checks as `Expr::Call`, and reject
//!    value-returning built-ins (`len`, `cap`, `append`, `make`, `new`,
//!    `complex`, `real`, `imag`, `min`, `max`) and type conversions (`int(x)`,
//!    `[]byte(s)`) that `go build` refuses in a `go` statement.
//! 2. Parallel assignment (`a, b = x, y`) must reject distinct fixed-size
//!    array lengths (`[3]int` vs `[4]int`), matching single-target assignment.
//! 3. Compound assignment (`+=`, `-=`, etc.) with a void call RHS must reject
//!    with `HYB-E1002` even when the LHS place has an unresolved type.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn task_spawn_invalid_callees_reject_e1002() {
    for (name, src) in [
        (
            "non-function local in task spawn",
            "fn main() {\n    x := 5\n    task x()\n}\n",
        ),
        (
            "local closure wrong arity in task spawn",
            "fn main() {\n    f := fn(a int) { _ = a }\n    task f(1, 2)\n}\n",
        ),
        (
            "non-function struct field in task spawn",
            "struct box_t { n int }\nfn main() {\n    b := box_t{n: 1}\n    task b.n()\n}\n",
        ),
        (
            "undefined struct method in task spawn",
            "struct box_t { n int }\nfn main() {\n    b := box_t{n: 1}\n    task b.missing()\n}\n",
        ),
        (
            "value-returning builtin len in task spawn",
            "fn main() {\n    task len(\"abc\")\n}\n",
        ),
        (
            "type conversion int(1) in task spawn",
            "fn main() {\n    task int(1)\n}\n",
        ),
        (
            "slice type conversion []byte in task spawn",
            "fn main() {\n    task []byte(\"abc\")\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "{name}: {:?}", r.diagnostics);
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "{name}: expected HYB-E1002, got {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn parallel_assign_sized_array_mismatch_rejects_e1002() {
    let src = "fn main() {\n    var a [3]int\n    var b [4]int\n    var n int\n    b, n = a, 1\n    _ = b\n    _ = n\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for parallel array length mismatch, got {:?}",
        r.diagnostics
    );
}

#[test]
fn compound_assign_void_call_rejects_e1002() {
    for src in [
        "fn noop() {}\nfn main() {\n    x := 1\n    x += noop()\n    _ = x\n}\n",
        "fn noop() {}\nfn main() {\n    var m map[string]any\n    m[\"k\"] += noop()\n    _ = m\n}\n",
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for void call in compound assignment, got {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn valid_task_spawn_and_parallel_array_assign_stay_accepted() {
    for (name, src) in [
        (
            "valid top-level fn spawn and parallel same-length array assign",
            "fn worker(a int) { _ = a }\nfn main() {\n    task worker(2)\n    var a [3]int\n    var b [3]int\n    var n int\n    b, n = a, 1\n    _ = b\n    _ = n\n}\n",
        ),
        (
            "shadowed builtin name as top-level fn in task spawn",
            "fn len(a int) { _ = a }\nfn main() {\n    task len(1)\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "{name}: {:?}", r.diagnostics);
        assert!(error_codes(&r).is_empty(), "{name}: {:?}", r.diagnostics);
    }
}
