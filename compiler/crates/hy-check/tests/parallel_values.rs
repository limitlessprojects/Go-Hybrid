//! parallel_values.rs — regression tests for #1922: every RHS in a parallel
//! binding/assignment is its own single-value slot.

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

const PRELUDE: &str = "fn void_fn() {\n}\n\nfn two() (int, int) {\n    return 1, 2\n}\n\n";

#[test]
fn void_call_in_parallel_binding_is_e1002() {
    let r = check(&format!(
        "{PRELUDE}fn main() {{\n    x, y := 1, void_fn()\n    _ = x + y\n}}\n"
    ));
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for a void RHS in `x, y := 1, void_fn()`, got {cs:?}"
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn void_call_in_parallel_assignment_is_e1002() {
    let r = check(&format!(
        "{PRELUDE}fn main() {{\n    var x int = 0\n    var y int = 0\n    x, y = 1, void_fn()\n    _ = x + y\n}}\n"
    ));
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for a void RHS in `x, y = 1, void_fn()`, got {cs:?}"
    );
}

#[test]
fn multi_value_call_in_parallel_binding_is_e2002() {
    // E2002 owns the multi-value shape in a binding slot (the assertion this
    // rule must not duplicate); the void case above is what E2002 cannot see.
    let r = check(&format!(
        "{PRELUDE}fn main() {{\n    x, y := 1, two()\n    _ = x + y\n}}\n"
    ));
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E2002"),
        "expected HYB-E2002 for a two-value RHS in a parallel binding, got {cs:?}"
    );
    assert!(
        !cs.iter().any(|c| c == "HYB-E1002"),
        "the multi-value binding shape belongs to E2002 alone, got {cs:?}"
    );
}

#[test]
fn legal_multi_value_binding_still_accepts() {
    // `a, b := two()` is exactly the shape Go allows; the new parallel check
    // must not touch it (names 2, values 1).
    let r = check(&format!("{PRELUDE}fn main() {{\n    a, b := two()\n    _ = a + b\n}}\n"));
    let cs = codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E1002"),
        "`a, b := two()` must stay accepted, got {cs:?}"
    );
}
