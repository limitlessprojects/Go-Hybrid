//! E3/E4 operand half of HYB-E2011 (SPEC-LANGUAGE §6.1).
//!
//! `?` is postfix on an error-returning call only. Before this pass,
//! `1?`, `x.n?`, `foo()?` when `foo` returns a non-error result, and
//! `opt()?` when `opt` returns `option[T]` all passed `hyb check` with 0
//! diagnostics and then failed `hyb build` / `go build` with an assignment
//! mismatch (GUARANTEES Row 13). The landing-spot half of E4 lives in
//! `try_context.rs` already; this pins the operand half.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).unwrap_or_else(|d| panic!("must parse: {d:?}"));
    check_file(&ast)
}

fn e2011(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error && d.code == "HYB-E2011")
        .map(|d| d.message.clone())
        .collect()
}

fn assert_operand_reject(src: &str, needle: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "must reject:\n{src}");
    let msgs = e2011(&r);
    assert!(
        msgs.iter().any(|m| m.contains(needle)),
        "expected HYB-E2011 containing {needle:?} for:\n{src}\ngot {msgs:?}"
    );
}

#[test]
fn non_call_operands_under_try_are_rejected() {
    for (src, needle) in [
        (
            "fn main() {\n    x := 1?\n    fmt.println(\"{x}\")\n}\n",
            "is not a call",
        ),
        (
            "fn main() {\n    x := \"hi\"?\n    fmt.println(\"{x}\")\n}\n",
            "is not a call",
        ),
        (
            "struct box { n int }\nfn main() {\n    b := box{n: 1}\n    x := b.n?\n    fmt.println(\"{x}\")\n}\n",
            "is not a call",
        ),
        (
            "fn main() {\n    x := none?\n    fmt.println(\"{x}\")\n}\n",
            "is not a call",
        ),
    ] {
        assert_operand_reject(src, needle);
    }
}

#[test]
fn non_error_and_option_calls_under_try_are_rejected() {
    assert_operand_reject(
        "fn foo() int { return 1 }\nfn main() {\n    x := foo()?\n    fmt.println(\"{x}\")\n}\n",
        "do not end in `error`",
    );
    assert_operand_reject(
        "fn foo() {}\nfn main() {\n    foo()?\n}\n",
        "do not end in `error`",
    );
    assert_operand_reject(
        "fn opt() option[int] { return none }\nfn main() {\n    x := opt()?\n    fmt.println(\"{x}\")\n}\n",
        "on `option`",
    );
    // Direct FnLit callee — TryWalker can see the signature (unlike a
    // local binding, which stays fail-open).
    assert_operand_reject(
        "fn main() {\n    x := (fn() int { return 1 })()?\n    fmt.println(\"{x}\")\n}\n",
        "do not end in `error`",
    );
}

/// Type params are the callee's, matching `results_end_in_error` after #3747.
/// A file-level fn is judged with its own params (so `fn bad[error any](z error) (int, error)`
/// does not count as an error-returning call). The enclosing function's params
/// must not leak into a different callee: `good` returning alias `failure = error`
/// stays an error call inside `outer[failure any]`. An `FnLit` has no param list;
/// its signature is judged with the enclosing params, same as `walk_fnlit`.
#[test]
fn try_operand_uses_callee_type_params() {
    assert_operand_reject(
        "fn bad[error any](z error) (int, error) {\n    return 1, z\n}\nfn main() {\n    x := bad(0)?\n    fmt.println(\"{x}\")\n}\n",
        "do not end in `error`",
    );
    assert_operand_reject(
        "type failure = error\nfn outer[failure any](z failure) (int, error) {\n    x := (fn() (int, failure) { return 1, z })()?\n    return x, nil\n}\nfn main() {\n    _, _ = outer(0)\n}\n",
        "do not end in `error`",
    );

    let accept_named = "type failure = error\nfn good() (int, failure) {\n    return 1, nil\n}\nfn outer[failure any](z failure) (int, error) {\n    _ = z\n    x := good()?\n    return x, nil\n}\nfn main() {\n    _, _ = outer(0)\n}\n";
    let r = check(accept_named);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "callee's own (empty) type params must not see the caller's `failure` param:\n{accept_named}\ngot {:?}",
        e2011(&r)
    );

    let accept_generic = "fn good[t any](z t) (int, error) {\n    _ = z\n    return 1, nil\n}\nfn main() {\n    x := good(1)?\n    fmt.println(\"{x}\")\n}\n";
    let r = check(accept_generic);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "generic callee whose params do not shadow `error` still accepts `?`:\n{accept_generic}\ngot {:?}",
        e2011(&r)
    );
}

#[test]
fn error_tailed_calls_still_accept() {
    for src in [
        "fn val() (int, error) { return 1, nil }\nfn main() {\n    x := val()?\n    fmt.println(\"{x}\")\n}\n",
        "fn only_err() error { return nil }\nfn main() {\n    only_err()?\n}\n",
        "fn main() {\n    f := fn() (int, error) { return 1, nil }\n    x := f()?\n    fmt.println(\"{x}\")\n}\n",
        "fn run() error {\n    step := fn() error { return nil }\n    step()?\n    return nil\n}\nfn main() { _ = run() }\n",
    ] {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "must accept:\n{src}\ngot {:?}",
            e2011(&r)
        );
    }
}
