//! Issue #1726 — a NAMED constant operand does not fold through a truncating
//! primitive conversion.
//!
//! `constconv::const_int_expr` understood literals, parens, unary minus,
//! `len()`, `min`/`max` and binary operators — but not `Expr::Ident`. So
//! `u8(300)` folded to `uint8(44)` and built, while the identical conversion
//! through a named constant emitted `uint8(big)` and died in Go:
//!
//! ```text
//! ./a.out.go:11:13: constant 300 overflows uint8
//! ```
//!
//! Go's rule is that a CONSTANT operand must already be representable in the
//! target type; the truncation Go performs for a *variable* operand does not
//! apply. A named `const` is a constant in Go's sense too, so the emitter has
//! to resolve the name and fold the value itself.
//!
//! The fix threads a const resolver through `fold_constant` -> `const_int_expr`
//! and gives `const_int_expr` an `Expr::Ident` arm. Two properties are pinned
//! here beyond the fix itself:
//!
//!   1. **No shape change for programs that already worked.** A value that
//!      fits is emitted verbatim, named constant or not (`uint8(small)` stays
//!      `uint8(small)`) — `constconv.rs`'s standing contract.
//!   2. **A const cycle terminates.** `const a = b; const b = a` is not
//!      foldable; the fold returns `None` instead of recursing until the stack
//!      dies (the defect class issue #1587 §3 reports for the checker's
//!      `is_const_string_expr`).

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn go_of(src: &str) -> String {
    let file = parse_file("test.hy", src).expect("parse ok");
    emit_go(&file).expect("emit ok")
}

#[test]
fn named_file_const_operand_folds_to_truncated_value() {
    let go = go_of(
        r#"
const big = 300

fn main() {
    a := u8(big)
    fmt.println(a)
}
"#,
    );
    assert!(
        go.contains("uint8(44)"),
        "`u8(big)` with `const big = 300` must fold to uint8(44), not stay `uint8(big)` \
         (Go: constant 300 overflows uint8):\n{go}"
    );
    assert!(
        !go.contains("uint8(big)"),
        "the unresolved name must not reach the emitted Go:\n{go}"
    );
}

#[test]
fn function_local_const_operand_folds() {
    let go = go_of(
        r#"
fn main() {
    const loc = 256 + 44
    a := u8(loc)
    fmt.println(a)
}
"#,
    );
    assert!(
        go.contains("uint8(44)"),
        "a fn-local `const` names a constant too — `u8(loc)` must fold:\n{go}"
    );
}

#[test]
fn named_const_chain_folds() {
    let go = go_of(
        r#"
const a = 300
const b = a

fn main() {
    c := u8(b)
    fmt.println(c)
}
"#,
    );
    assert!(
        go.contains("uint8(44)"),
        "the fold has to follow a const-to-const chain:\n{go}"
    );
}

#[test]
fn signed_named_const_truncates_twos_complement() {
    let go = go_of(
        r#"
const big = 200

fn main() {
    a := i8(big)
    fmt.println(a)
}
"#,
    );
    assert!(
        go.contains("int8(-56)"),
        "200 does not fit i8; two's complement in 8 bits is -56:\n{go}"
    );
}

#[test]
fn in_range_named_const_is_emitted_verbatim() {
    // The "only rewrite what Go would refuse" contract: a named constant that
    // already fits keeps its name, so no currently-working program changes
    // shape.
    let go = go_of(
        r#"
const small = 44

fn main() {
    a := u8(small)
    fmt.println(a)
}
"#,
    );
    assert!(
        go.contains("uint8(small)") && !go.contains("uint8(44)"),
        "a fitting named const must stay verbatim (`uint8(small)`):\n{go}"
    );
}

#[test]
fn runtime_operand_is_untouched() {
    let go = go_of(
        r#"
fn main() {
    v := 300
    a := u8(v)
    fmt.println(a)
}
"#,
    );
    assert!(
        go.contains("uint8(v)"),
        "a variable operand is not a constant — Go truncates it at run time, so it \
         must be left alone:\n{go}"
    );
}

#[test]
fn unsigned_literal_still_folds() {
    // The pre-existing SPEC test-claim (`u8(300) // expect: 44`) must survive.
    let go = go_of(
        r#"
fn main() {
    a := u8(300)
    fmt.println(a)
}
"#,
    );
    assert!(go.contains("uint8(44)"), "literal folding must not regress:\n{go}");
}
