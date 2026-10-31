//! composite_nils_n1.rs — regression tests for #1987: `check_composite_nils`
//! bypassed Rule N1 (`HYB-E4001`) on five literal shapes.
//!
//! Each test below is one of the report's five causes. The negative test pins
//! that the widening did not turn `error`'s legitimate `nil` into an error.

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

fn assert_e4001(src: &str, what: &str) {
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E4001"),
        "expected HYB-E4001 for {what}, got {cs:?}"
    );
    assert_eq!(r.verdict, Verdict::Reject, "{what} must reject");
}

#[test]
fn map_literal_key_nil_is_e4001() {
    // cause 1: only the value slot was classified
    assert_e4001(
        "fn main() {\n    m := map[*int]int{nil: 1}\n    _ = m\n}\n",
        "a nil map key",
    );
}

#[test]
fn parenthesized_array_literal_nil_is_e4001() {
    // cause 2: `as_array_lit()` was called on the Paren, not its inner literal
    assert_e4001(
        "fn main() {\n    a := ([1][]int{nil})\n    _ = a\n}\n",
        "a nil inside a parenthesized array literal",
    );
}

#[test]
fn generic_struct_field_nil_is_e4001() {
    // cause 3: `box[[]int]` missed the base-name `structs` lookup, so the
    // field sweep never ran and no substitution happened
    assert_e4001(
        "struct box[t] {\n    v t\n}\n\nfn main() {\n    b := box[[]int]{v: nil}\n    _ = b\n}\n",
        "a nil in a generic struct field",
    );
}

#[test]
fn move_wrapped_composite_nil_is_e4001() {
    // cause 4: `Expr::Move` hit the catch-all arm
    assert_e4001(
        "fn main() {\n    ch := make(chan [][]int, 1)\n    ch <- move [][]int{nil}\n}\n",
        "a nil inside a move-wrapped composite literal",
    );
}

#[test]
fn some_wrapped_composite_nil_is_e4001() {
    // cause 5: `some(...)` hit the catch-all arm
    assert_e4001(
        "struct job {\n    p []int\n}\n\nfn main() {\n    o := some(job{p: nil})\n    _ = o\n}\n",
        "a nil inside a some()-wrapped struct literal",
    );
}

#[test]
fn nil_for_error_is_still_accepted() {
    // N1 keeps exactly one home for nil: `error`. Widening the composite
    // sweep must not close that door.
    let r = check("fn main() {\n    var e error = nil\n    _ = e\n}\n");
    let cs = codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E4001"),
        "nil for `error` must remain valid, got {cs:?}"
    );
}
