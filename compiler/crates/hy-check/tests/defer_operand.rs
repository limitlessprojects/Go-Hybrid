//! Issue #288 integration: the HYB-E2001 defer-operand rule has ONE owner
//! (builtins.rs `check_stmt_defer_call`) and is wired into BOTH walks — the
//! Tier-0 flat walk (per flattened statement) and the closure walk
//! (closures.rs `check_stmt_boundaries`), because FnLit bodies sit outside
//! the flat walk (the A5 perimeter).
//!
//! Pins the three behaviors the wave-2 triage identified as what main
//! genuinely lacked, plus the dedupe bar:
//!   1. a `defer 3` inside a closure body is rejected (main passed it with
//!      0 diagnostics);
//!   2. the double-E2001 dedupe: exactly ONE diagnostic per deferred
//!      non-call (the merged tree used to fire both the builtins pass and a
//!      second pass on the same statement);
//!   3. the landed Try-operand policy: `defer f.close()?` (a Try-wrapping
//!      call) is the documented legal shape and stays legal, in function
//!      bodies AND closure bodies.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn e2001_count(r: &CheckResult) -> usize {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error && d.code == "HYB-E2001")
        .count()
}

#[test]
fn defer_non_call_in_closure_body_rejected() {
    // Gap (1) from the wave-2 triage: the builtins pass does not walk
    // closure bodies, so `defer 3` one scope in passed main with 0
    // diagnostics and failed late in the emitter. The closure wiring fires
    // the same one-owner rule here.
    let r = check("fn main() {\n    f := fn() {\n        defer 3\n    }\n    f()\n}\n");
    assert_eq!(e2001_count(&r), 1, "{:?}", r.diagnostics);
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn defer_non_call_flat_body_rejected_exactly_once() {
    // Dedupe bar: `defer 3` in a function body must produce exactly ONE
    // E2001 (the merged tree fired both the builtins pass and a second
    // wiring on the same statement — verified via --json during triage).
    let r = check("fn main() {\n    defer 3\n}\n");
    assert_eq!(e2001_count(&r), 1, "{:?}", r.diagnostics);

    let r = check("fn main() {\n    x := 1\n    defer x + 1\n}\n");
    assert_eq!(e2001_count(&r), 1, "{:?}", r.diagnostics);
}

#[test]
fn defer_non_call_in_nested_closure_block_rejected() {
    // The closure walk recurses into nested blocks — a defer inside an `if`
    // inside a closure is still covered.
    let r = check(
        "fn main() {\n    f := fn() {\n        if true {\n            defer 3\n        }\n    }\n    f()\n}\n",
    );
    assert_eq!(e2001_count(&r), 1, "{:?}", r.diagnostics);
}

#[test]
fn try_wrapping_call_operand_is_the_legal_shape() {
    // Landed policy (builtins.rs unwraps Paren/Try): `defer f.close()?` is
    // the documented legal operand shape — the wave-2 triage explicitly
    // reconciled this PR's earlier stricter reading to main's.
    let r = check(
        "fn cleanup() error {\n    return nil\n}\n\nfn main() {\n    defer cleanup()?\n    fmt.println(\"done\")\n}\n",
    );
    assert_eq!(e2001_count(&r), 0, "{:?}", r.diagnostics);
}

#[test]
fn try_wrapping_call_operand_legal_inside_closure_too() {
    let r = check(
        "fn cleanup() error {\n    return nil\n}\n\nfn main() {\n    f := fn() {\n        defer cleanup()?\n    }\n    f()\n}\n",
    );
    assert_eq!(e2001_count(&r), 0, "{:?}", r.diagnostics);
}

#[test]
fn defer_call_still_legal_everywhere() {
    for src in [
        "fn zap() error {\n    return nil\n}\n\nfn main() {\n    defer zap()\n}\n",
        "fn zap() error {\n    return nil\n}\n\nfn main() {\n    defer (zap())\n}\n",
        "fn main() {\n    ch := make(chan int, 1)\n    defer close(ch)\n}\n",
        "fn main() {\n    f := fn() {\n        defer close(nil)\n    }\n    f()\n}\n",
    ] {
        let r = check(src);
        assert_eq!(
            e2001_count(&r),
            0,
            "legal defer shape must pass: {:?}\n{}",
            r.diagnostics,
            src
        );
    }
}
