//! Issue #4030 (part 1): an array length that mentions a runtime binding in a
//! non-identifier expression form is not a Go constant.
//!
//! `check_array_len` handled four shapes: a folded constant, a non-integer
//! syntax, a bare identifier, and `len`/`cap` over a non-constant operand.
//! An expression like `n + 1` matched **none** of them — the constant folder
//! returns `None` because `n` is a parameter, `is_non_int_array_len_expr` is
//! false because `+` yields an integer — so the function returned having emitted
//! nothing:
//!
//! ```hybrid
//! fn foo(n int, a [n + 1]int) { _ = a }
//! ```
//!
//! `hyb check` printed `OK (0 diagnostics)`; `go build` said `undefined: n`.
//!
//! What this file pins:
//!   1. the parameter-in-length form is rejected (E6004) in a **parameter** type
//!      and in a **result** type;
//!   2. a file-level `const` in the same expression shape stays accepted, so the
//!      new branch did not turn into "any `+` is an error";
//!   3. a plain literal length stays accepted;
//!   4. the pre-existing bare-identifier behaviour is unchanged — `[n]int` with
//!      a parameter `n` was already E6004 and must stay E6004, not become a
//!      different code or a duplicate.

use hy_check::{check_file, CheckResult, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn has_e6004(r: &CheckResult) -> bool {
    r.diagnostics.iter().any(|d| d.code == "HYB-E6004")
}

#[test]
fn param_in_array_length_expression_rejected() {
    // The exact repro.
    let r = check(
        "fn foo(n int, a [n + 1]int) {\n    _ = a\n}\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        has_e6004(&r),
        "expected E6004 for a non-constant array length, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn param_in_result_array_length_rejected() {
    let r = check("fn bar(n int) [n + 1]int {\n    return [4]int{}\n}\n");
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(has_e6004(&r), "expected E6004, got: {:?}", r.diagnostics);
}

#[test]
fn const_in_array_length_expression_accepted() {
    // Guard against over-rejection: the same shape with a foldable const is a
    // perfectly good Go array length.
    let r = check(
        "const k = 3\n\nfn good(n int, a [k + 1]int) int {\n    _ = a\n    return n\n}\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn literal_array_length_accepted() {
    let r = check("fn f(a [3]int) {\n    _ = a\n}\n");
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn bare_identifier_param_length_still_e6004() {
    // Pre-existing behaviour: `[n]int` with a parameter was already reported.
    // The new branch runs only for non-identifier expressions, so this must be
    // exactly one E6004, not two and not a different code.
    let r = check("fn foo(n int, a [n]int) {\n    _ = a\n}\n");
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    let n = r
        .diagnostics
        .iter()
        .filter(|d| d.code == "HYB-E6004")
        .count();
    assert_eq!(n, 1, "expected exactly one E6004, got {}: {:?}", n, r.diagnostics);
}
