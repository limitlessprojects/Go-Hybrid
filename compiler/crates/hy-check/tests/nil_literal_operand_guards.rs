//! nil_literal_operand_guards.rs — issue #1777: a literal `nil` is not a valid
//! operand of any operator except the equality surface (`==`, `!=`).
//!
//! Root cause: `kind_of_expr` maps `LitKind::Nil` to `OpKind::Unknown` and
//! `Unknown` is deliberately permissive in the operand gates, so shift,
//! arithmetic, bitwise, logical and unary operators all failed open on a bare
//! `nil` literal. Every guard here is `HYB-E1002` (rule `BUILDABLE`) — no new
//! registry code, so the checker-corpus counters do not move.
//!
//! Layering: this file is the OPERAND layer. Decl-level nil (`x := nil`) stays
//! E4003, ascription-level (N1) stays E4001, equality/nilability surfaces are
//! pinned by the positive controls at the bottom.

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("parses");
    check_file(&ast)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn assert_e1002(src: &str) {
    let cs = codes(src);
    assert!(
        cs.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 in {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(
        cs.is_empty(),
        "expected 0 diagnostics, got {cs:?} for:\n{src}"
    );
}

// ---------------------------------------------------------------------------
// Issue #1777 reproduction surface: shift operands
// ---------------------------------------------------------------------------

#[test]
fn rejects_shift_count_nil() {
    assert_e1002("fn main() {\n    y := 1 << nil\n    _ = y\n}\n");
}

#[test]
fn rejects_sr_shift_count_nil() {
    assert_e1002("fn main() {\n    y := 1 >> nil\n    _ = y\n}\n");
}

#[test]
fn rejects_nil_shifted_operand() {
    assert_e1002("fn main() {\n    y := nil << 1\n    _ = y\n}\n");
    assert_e1002("fn main() {\n    y := nil >> 2\n    _ = y\n}\n");
}

// ---------------------------------------------------------------------------
// The same Unknown funnel on the arithmetic / bitwise / logical arms
// ---------------------------------------------------------------------------

#[test]
fn rejects_arithmetic_nil_operands() {
    assert_e1002("fn main() {\n    y := 1 + nil\n    _ = y\n}\n");
    assert_e1002("fn main() {\n    y := 1 - nil\n    _ = y\n}\n");
    assert_e1002("fn main() {\n    y := nil * 3\n    _ = y\n}\n");
    assert_e1002("fn main() {\n    y := nil / 3\n    _ = y\n}\n");
}

#[test]
fn rejects_bitwise_and_mod_nil_operands() {
    assert_e1002("fn main() {\n    y := 1 % nil\n    _ = y\n}\n");
    assert_e1002("fn main() {\n    y := nil & 2\n    _ = y\n}\n");
    assert_e1002("fn main() {\n    y := nil | 2\n    _ = y\n}\n");
    assert_e1002("fn main() {\n    y := nil ^ 2\n    _ = y\n}\n");
}

#[test]
fn rejects_logical_nil_operands() {
    // The Bool gate would fire only on a CONCRETE non-bool kind; a nil
    // literal is `Unknown` on both sides, so `nil && false` sailed through.
    assert_e1002("fn main() {\n    y := nil && false\n    _ = y\n}\n");
    assert_e1002("fn main() {\n    y := false || nil\n    _ = y\n}\n");
}

// ---------------------------------------------------------------------------
// Unary operators (`-`, `!`) — `&nil` was already closed (non-addressable)
// ---------------------------------------------------------------------------

#[test]
fn rejects_unary_neg_and_not_on_nil() {
    assert_e1002("fn main() {\n    y := -nil\n    _ = y\n}\n");
    assert_e1002("fn main() {\n    y := !nil\n    _ = y\n}\n");
    // No double report: `+nil` and `^nil` already reject at the source level.
}

// ---------------------------------------------------------------------------
// Compound assignment with a nil value: the belt only covers the
// N1-silent (`Unknown`-target) shape; known-shaped targets keep E4001 —
// a second E1002 there would be a duplicate report, so these stay absent.
// ---------------------------------------------------------------------------

#[test]
fn known_target_compound_nil_still_e4001_only() {
    // Primitive target: N1 fires, and the belt must NOT add a second diag.
    let cs = codes("fn main() {\n    xl := 8\n    xl += nil\n    println(xl)\n}\n");
    assert!(
        cs.iter().filter(|c| *c == "HYB-E4001").count() == 1,
        "expected exactly one E4001, got {cs:?}"
    );
    assert!(
        !cs.contains(&"HYB-E1002".to_string()),
        "belt must not duplicate the N1 report on a known target: {cs:?}"
    );
}

// ---------------------------------------------------------------------------
// Positive controls — the nil surfaces that MUST keep accepting
// ---------------------------------------------------------------------------

#[test]
fn acceptable_nil_surfaces_unchanged() {
    // Equality against a nilable value (the Eq/Ne carve-out).
    assert_ok("fn main() {\n    x := 5\n    p := &x\n    if p == nil {}\n    if p != nil {}\n}\n");
    // None of the operand guards may touch comparisons with nil.
    assert_ok(
        "fn main() {\n    x := 5\n    p := &x\n    if p == nil { println(1) } else { println(2) }\n}\n",
    );
    // Plain assignment of nil is owned by N1/E4001-eligible shapes, not by
    // this family — no operand guard may fire on it.
    let cs = codes("fn main() {\n    x := 5\n    p := &x\n    if p == nil { println(1) }\n}\n");
    assert!(cs.is_empty(), "unexpected diagnostics: {cs:?}");
}
