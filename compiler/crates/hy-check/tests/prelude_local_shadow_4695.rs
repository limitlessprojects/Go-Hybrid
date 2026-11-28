//! prelude_local_shadow_4695.rs — regression for issue #4695 variant E:
//! a LOCAL binding named `hy_pop` captures the lowered `.pop()` call.
//!
//! `.pop()` on a slice lowers to a call of the prelude helper `hy_pop(&s)`
//! INSIDE the user's function body (hy-emit/src/expr.rs), so a local
//! `hy_pop` in scope shadows the package-level helper and the emitted Go
//! fails to build ("cannot call hy_pop (variable of type int)"). Top-level
//! declarations are a different shape (issue #777, rivals #819/#825/#826);
//! this file covers ONLY locals at helper call sites, which none of them
//! touches. All such collisions reject fail-closed as HYB-E1002 (rule
//! `BUILDABLE`, GUARANTEES.md Row 13).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "test_4695.hy";
    let ast =
        parse_file(path, src).unwrap_or_else(|diags| panic!("snippet must parse: {:?}", diags));
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
    assert_eq!(r.verdict, Verdict::Reject, "expected reject for {}", what);
}

fn assert_accepts(r: &hy_check::CheckResult, what: &str) {
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept for {}, got {:?} with diagnostics {:?}",
        what,
        r.verdict,
        r.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| d.code.clone())
            .collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// Reject: #4695 variant E, verbatim
// ---------------------------------------------------------------------------

#[test]
fn test_local_hypop_captures_pop() {
    let src = r#"
fn main() {
    hy_pop := 1
    stack := []int{1, 2, 3}
    popped := stack.pop()
    fmt.println("{hy_pop} {popped}")
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "local hy_pop + .pop() in the same body (#4695 E)");
}

// ---------------------------------------------------------------------------
// Reject: local in an ENCLOSING block, .pop() in an inner block
// ---------------------------------------------------------------------------

#[test]
fn test_enclosing_block_local_captures_pop() {
    let src = r#"
fn main() {
    hy_pop := 1
    stack := []int{1, 2, 3}
    if true {
        popped := stack.pop()
        fmt.println("{hy_pop} {popped}")
    }
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "outer-block local hy_pop + .pop() in inner block");
}

// ---------------------------------------------------------------------------
// Reject: a PARAMETER named hy_pop
// ---------------------------------------------------------------------------

#[test]
fn test_param_hypop_captures_pop() {
    let src = r#"
fn f(hy_pop int) int {
    stack := []int{1, 2, 3}
    popped := stack.pop()
    return popped + hy_pop
}

fn main() {
    fmt.println("{f(1)}")
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "param hy_pop + .pop() in the body");
}

// ---------------------------------------------------------------------------
// Reject: .pop() on a slice FIELD, local hy_pop in scope
// ---------------------------------------------------------------------------

#[test]
fn test_field_base_pop_captures_local() {
    let src = r#"
struct holder {
    items []int
}

fn main() {
    hy_pop := 1
    h := holder{items: []int{1, 2, 3}}
    popped := h.items.pop()
    fmt.println("{hy_pop} {popped}")
}
"#;
    let r = check(src);
    assert_rejects_e1002(&r, "local hy_pop + .pop() on a slice field");
}

// ---------------------------------------------------------------------------
// Accept: local declared AFTER the .pop() call (Go scoping: no capture)
// ---------------------------------------------------------------------------

#[test]
fn test_local_hypop_after_pop_call_accepts() {
    let src = r#"
fn main() {
    stack := []int{1, 2, 3}
    popped := stack.pop()
    hy_pop := 1
    fmt.println("{hy_pop} {popped}")
}
"#;
    let r = check(src);
    assert_accepts(&r, "local hy_pop declared after the .pop() call");
}

// ---------------------------------------------------------------------------
// Accept: local in a SIBLING block (scope popped before the call)
// ---------------------------------------------------------------------------

#[test]
fn test_local_hypop_in_sibling_block_accepts() {
    let src = r#"
fn main() {
    stack := []int{1, 2, 3}
    if true {
        hy_pop := 1
        fmt.println("{hy_pop}")
    }
    popped := stack.pop()
    fmt.println("{popped}")
}
"#;
    let r = check(src);
    assert_accepts(&r, "local hy_pop confined to a sibling if-block");
}

// ---------------------------------------------------------------------------
// Accept: local hy_pop with NO .pop() use anywhere (nothing to capture)
// ---------------------------------------------------------------------------

#[test]
fn test_local_hypop_without_pop_accepts() {
    let src = r#"
fn main() {
    hy_pop := 1
    stack := []int{1, 2, 3}
    fmt.println("{hy_pop} {stack[0]}")
}
"#;
    let r = check(src);
    assert_accepts(&r, "local hy_pop but no .pop() call in scope");
}

// ---------------------------------------------------------------------------
// Accept: plain .pop() baseline (no shadowing local)
// ---------------------------------------------------------------------------

#[test]
fn test_plain_pop_baseline_accepts() {
    let src = r#"
fn main() {
    stack := []int{1, 2, 3}
    popped := stack.pop()
    fmt.println("{popped}")
}
"#;
    let r = check(src);
    assert_accepts(&r, "plain .pop() with no local named hy_pop");
}
