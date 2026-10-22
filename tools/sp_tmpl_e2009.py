#!/usr/bin/env python3
"""sp_tmpl_e2009.py — T-E2009 template family (bake-off arm A0, issue #827).

CLASS-LEVEL transplants only:
  * non_integer_bound_kind gains the missing non-integer shapes: nil,
    bool-result operators (comparisons / logical / `!`), AddrOf, and the
    predeclared `nil`/`true`/`false` spellings (#928-era shape discipline).
  * #998 pattern — untyped const identifiers resolve through the file
    const map and classify by their initializer literal (ty_of_expr sees
    nothing for `b.ty == None` bindings). Block-scoped consts stay
    fail-open (FnState carries no const-value map — #998 precedent).
  * the lowering-derived rules (issue body, hy-emit behavior):
      exclusive `for i in lo..hi` -> `i := lo; i < hi; i++`
      inclusive `lo..=hi` -> `hyLo := lo; hyHi := hi; i, hyCont := ...`
    so bounds mismatch when they resolve to different fixed-width integer
    types (untyped literals/consts adapt; the synthesized slots default to
    `int`), and constant bounds must fit the counter's type.

Completion contract = all four families the issue enumerates.
"""

from __future__ import annotations

RI = "compiler/crates/hy-check/src/range_iter.rs"

TEST_PATH = "compiler/crates/hy-check/tests/range_bound_type_forms.rs"
TEST_STEM = "range_bound_type_forms"

TEST_SRC = '''//! #827 — non-integer, const, mismatched, and overflowing range bounds.
//!
//! The emitter lowers `for i in lo..hi` to `for i := lo; i < hi; i++`
//! (and `lo..=hi` to `hyLo := lo; hyHi := hi; i, hyCont := hyLo,
//! hyLo <= hyHi; ...`), so Go rejects: non-integer bounds (`0..nil`,
//! `0..(1 < 2)`, `(!b)..5`, `0..(&x)`), untyped const float/bool/string
//! bounds (`0..f` on `const f = 2.5`), bounds resolving to different
//! fixed-width integer types (`0..n` on `var n u32`, `lo i32..hi i64`,
//! `lo i64..=10`), and constant bounds that overflow the counter's type
//! (`0..(1 << 63)`, `lo i8..300`). Untyped literals and untyped consts
//! adapt to the counter and stay legal.

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

fn assert_code(src: &str, want: &str) {
    let cs = codes(src);
    assert!(
        !cs.is_empty() && cs.iter().all(|c| c == want),
        "expected only [{want}], got {cs:?} for:\\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(cs.is_empty(), "expected 0 diagnostics, got {cs:?} for:\\n{src}");
}

#[test]
fn nil_and_bool_result_bounds_reject() {
    assert_code(
        "fn main() {\\n    for i in 0..nil { _ = i }\\n}\\n",
        "HYB-E2009",
    );
    assert_code(
        "fn main() {\\n    for i in 0..(1 < 2) { _ = i }\\n}\\n",
        "HYB-E2009",
    );
    assert_code(
        "fn main() {\\n    b := true\\n    for i in (!b)..5 { _ = i }\\n}\\n",
        "HYB-E2009",
    );
}

#[test]
fn untyped_const_non_integer_bounds_reject() {
    assert_code(
        "const f = 2.5\\n\\nfn main() {\\n    for i in 0..f { _ = i }\\n}\\n",
        "HYB-E2009",
    );
    assert_code(
        "const b = true\\n\\nfn main() {\\n    for i in 0..b { _ = i }\\n}\\n",
        "HYB-E2009",
    );
}

#[test]
fn mismatched_integer_bound_types_reject() {
    assert_code(
        "fn main() {\\n    var n u32 = 5\\n    for i in 0..n { _ = i }\\n}\\n",
        "HYB-E2009",
    );
    assert_code(
        "fn main() {\\n    var lo i32 = 0\\n    var hi i64 = 5\\n    for i in lo..hi { _ = i }\\n}\\n",
        "HYB-E2009",
    );
    assert_code(
        "fn main() {\\n    var lo i64 = 0\\n    for i in lo..=10 { _ = i }\\n}\\n",
        "HYB-E2009",
    );
}

#[test]
fn overflowing_constant_bounds_reject() {
    assert_code(
        "fn main() {\\n    for i in 0..(1 << 63) { _ = i }\\n}\\n",
        "HYB-E2009",
    );
    assert_code(
        "fn main() {\\n    for i in 0..99999999999999999999 { _ = i }\\n}\\n",
        "HYB-E2009",
    );
    assert_code(
        "fn main() {\\n    var lo i8 = 0\\n    for i in lo..300 { _ = i }\\n}\\n",
        "HYB-E2009",
    );
}

#[test]
fn valid_integer_ranges_stay_legal() {
    // typed lo + untyped hi: the constant adapts (issue accept control)
    assert_ok(
        "fn main() {\\n    var lo i64 = 0\\n    for i in lo..10 { _ = i }\\n}\\n",
    );
    // conversion lo matching the typed hi (issue accept control)
    assert_ok(
        "fn main() {\\n    var n u32 = 5\\n    for i in u32(0)..n { _ = i }\\n}\\n",
    );
    // plain literal ranges
    assert_ok("fn main() {\\n    for i in 0..10 { _ = i }\\n    for i in 0..=8 { _ = i }\\n}\\n");
    // len() is int
    assert_ok(
        "fn main() {\\n    xs := []int{1, 2}\\n    for i in 0..len(xs) { _ = i }\\n}\\n",
    );
    // untyped const bounds adapt
    assert_ok(
        "const n = 5\\n\\nfn main() {\\n    for i in 0..n { _ = i }\\n}\\n",
    );
    // same-type typed bounds
    assert_ok(
        "fn main() {\\n    var lo i64 = 0\\n    var hi i64 = 5\\n    for i in lo..hi { _ = i }\\n}\\n",
    );
    // runtime int bound
    assert_ok(
        "fn main() {\\n    var n int = 5\\n    for i in 0..n { _ = i }\\n}\\n",
    );
}
'''

FIXTURE_PATH = "tests/checker-corpus/range-bound-type-forms-reject.hy"
FIXTURE_SRC = '''// ============================================================================
// range-bound-type-forms-reject — Checker corpus (issue #827).
//
// PURPOSE: `for i in lo..hi` bounds that pass the shape fallback but fail
// the lowered Go: non-integer bounds, untyped const non-integer bounds,
// mismatched fixed-width integer types, and constants overflowing the
// counter's type.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — the emitter lowers
// ranges to `i := lo; i < hi; i++`; Go refuses every shape below
// (`mismatched types ...` / `constant ... overflows ...` /
// `truncated to int`).
//
// ---- Why this is a REJECT ----
// `non_integer_bound_kind` missed nil / bool-result operators / AddrOf,
// untyped const identifiers carry no type for `ty_of_expr`, and no rule
// compared the two bounds' integer types or folded constants against the
// counter's range. The fix extends the kind classifier, resolves untyped
// consts through the file const map (#998 pattern), and adds the
// lowering-derived mismatch/overflow rules to the Range arm. Untyped
// literals and untyped consts adapt and stay legal.
//
// machine-readable verdict marker below; regression pins live in
// `hy-check/tests/range_bound_type_forms.rs`.
// ============================================================================

const f = 2.5

fn main() {
    for i in 0..nil { _ = i }
    for i in 0..(1 < 2) { _ = i }
    for i in 0..f { _ = i }

    var n u32 = 5
    for i in 0..n { _ = i }

    var lo i32 = 0
    var hi i64 = 5
    for i in lo..hi { _ = i }

    for i in 0..(1 << 63) { _ = i }
}

// EXPECTED-VERDICT: REJECT BUILDABLE
'''


def candidates():
    return [
        {
            "name": "full",
            "edits": _edits(),
            "new_files": [(TEST_PATH, TEST_SRC)],
            "rust_tests": ["--test", TEST_STEM],
            "rationale": (
                "T-E2009 full: non_integer_bound_kind gains nil / "
                "bool-result ops / AddrOf / predeclared spellings; untyped "
                "const idents resolve through the file const map (#998 "
                "pattern); the Range arm gains the lowering-derived "
                "mismatch + overflow rules. Untyped literals/consts adapt "
                "and stay legal. Block-scoped consts stay fail-open."
            ),
        },
    ]


def _edits():
    edits = []

    # (1) the Range arm: destructure `inclusive` and run the lowering-
    # derived mismatch/overflow rules.
    edits.append((RI,
        """            Stmt::For {
                kind: ForKind::Range { var, lo, hi, .. },
                ..
            } => {
                self.check_integer_range_bound(st, var, lo);
                self.check_integer_range_bound(st, var, hi);
            }""",
        """            Stmt::For {
                kind: ForKind::Range {
                    var,
                    lo,
                    hi,
                    inclusive,
                    ..
                },
                ..
            } => {
                self.check_integer_range_bound(st, var, lo);
                self.check_integer_range_bound(st, var, hi);
                // Issue #827: the lowering-derived rules — the bounds must
                // type-check as a comparison after `i := lo; i < hi` (and
                // the inclusive `hyLo`/`hyHi` form), and constant bounds
                // must fit the counter's integer type.
                self.check_range_bound_types(st, var, lo, hi, *inclusive);
            }"""))

    # (2) nil literal + predeclared spellings + AddrOf inside the kind match.
    edits.append((RI,
        """            Expr::Lit {
                kind: LitKind::Str(_) | LitKind::RawStr(_),
                ..
            }
            | Expr::Interp { .. } => return Some("string".to_string()),""",
        """            Expr::Lit {
                kind: LitKind::Str(_) | LitKind::RawStr(_),
                ..
            }
            | Expr::Interp { .. } => return Some("string".to_string()),
            Expr::Lit {
                kind: LitKind::Nil,
                ..
            } => return Some("nil".to_string()),
            Expr::Ident { name, .. }
                if matches!(name.as_str(), "nil" | "true" | "false") =>
            {
                return Some(name.to_string());
            }"""))

    # (3) `!x` yields bool — never an integer bound.
    edits.append((RI,
        """            Expr::Unary { operand, .. } => {
                if let Some(k) = self.non_integer_bound_kind(st, operand) {
                    return Some(k);
                }
            }""",
        """            Expr::Unary { op, operand, .. } => {
                // Issue #827: `!x` yields a bool — never an integer bound.
                if matches!(op, hy_syntax::UnOp::Not) {
                    return Some("bool".to_string());
                }
                if let Some(k) = self.non_integer_bound_kind(st, operand) {
                    return Some(k);
                }
            }"""))

    # (4) comparison/logical operators yield bool; `&x` is a pointer.
    edits.append((RI,
        """            Expr::Binary { lhs, rhs, .. } => {
                if let Some(k) = self
                    .non_integer_bound_kind(st, lhs)
                    .or_else(|| self.non_integer_bound_kind(st, rhs))
                {
                    return Some(k);
                }
            }
            _ => {}""",
        """            Expr::Binary { op, lhs, rhs, .. } => {
                // Issue #827: comparison/logical operators yield a bool —
                // `0..(1 < 2)` is `mismatched types` in the lowered Go.
                if matches!(
                    op,
                    hy_syntax::BinOp::Eq
                        | hy_syntax::BinOp::Ne
                        | hy_syntax::BinOp::Lt
                        | hy_syntax::BinOp::Le
                        | hy_syntax::BinOp::Gt
                        | hy_syntax::BinOp::Ge
                        | hy_syntax::BinOp::And
                        | hy_syntax::BinOp::Or
                ) {
                    return Some("bool".to_string());
                }
                if let Some(k) = self
                    .non_integer_bound_kind(st, lhs)
                    .or_else(|| self.non_integer_bound_kind(st, rhs))
                {
                    return Some(k);
                }
            }
            Expr::AddrOf { .. } => {
                // Issue #827: `&x` is a pointer — never an integer bound.
                return Some("pointer".to_string());
            }
            _ => {}"""))

    # (5) untyped const identifiers resolve through the file const map
    # (#998 pattern) — before the shape fallback that sees nothing.
    edits.append((RI,
        """        let shape = self.shape_of_expr(st, expr);
        match shape {
            TypeShape::Primitive | TypeShape::Unknown => None,
            other => Some(other.render()),
        }
    }""",
        """        // Issue #827: an untyped const identifier carries no type
        // ascription, so `ty_of_expr`/`shape_of_expr` see nothing. Resolve
        // the initializer through the file const map (#998 pattern) and
        // classify the literal. Block-scoped consts stay fail-open (no
        // const-value map in FnState — the #998 precedent).
        if let Expr::Ident { name, .. } = unwrap_parens(expr) {
            if st.env_lookup(name).is_none() {
                if let Some(init) = self.const_values.get(name.as_str()) {
                    match unwrap_parens(init) {
                        Expr::Lit {
                            kind: LitKind::Float(_),
                            ..
                        } => return Some("float".to_string()),
                        Expr::Lit {
                            kind: LitKind::Bool(_),
                            ..
                        } => return Some("bool".to_string()),
                        Expr::Lit {
                            kind: LitKind::Str(_) | LitKind::RawStr(_),
                            ..
                        } => return Some("string".to_string()),
                        Expr::Lit {
                            kind: LitKind::Nil,
                            ..
                        } => return Some("nil".to_string()),
                        _ => {}
                    }
                }
            }
        }
        let shape = self.shape_of_expr(st, expr);
        match shape {
            TypeShape::Primitive | TypeShape::Unknown => None,
            other => Some(other.render()),
        }
    }

    /// Issue #827: the lowering-derived range rules. Exclusive
    /// `for i in lo..hi` lowers to `i := lo; i < hi; i++`; inclusive
    /// `lo..=hi` lowers to `hyLo := lo; hyHi := hi; i, hyCont := hyLo,
    /// hyLo <= hyHi; ...`. The comparison type-mismatches when the two
    /// sides resolve to different fixed-width integer types — including
    /// the untyped-defaults-to-`int` rule for the synthesized slots.
    /// Constant bounds must fit the counter's type.
    fn check_range_bound_types(
        &mut self,
        st: &FnState,
        var: &str,
        lo: &Expr,
        hi: &Expr,
        inclusive: bool,
    ) {
        let tl = self.typed_int_bound(st, lo);
        let th = self.typed_int_bound(st, hi);
        let mismatch = match (&tl, &th) {
            (Some(a), Some(b)) => a != b,
            (None, Some(b)) => b != "int",
            (Some(a), None) => inclusive && a != "int",
            (None, None) => false,
        };
        if mismatch {
            let ta = tl.clone().unwrap_or_else(|| "untyped (adapts to int)".to_string());
            let tb = th.clone().unwrap_or_else(|| "untyped (adapts to int)".to_string());
            let span = lo.span();
            let msg = format!(
                "range bounds `{}` ({ta}) and `{}` ({tb}) lower to mismatched integer types — `for {var} in lo..hi` must type-check after the lowering (LX13) (HYB-E2009)",
                lo.render(),
                hi.render()
            );
            let notes = vec![
                "why: the emitter lowers `for i in lo..hi` to `i := lo; i < hi; i++` — Go rejects a comparison over mismatched integer types, so the emitted module would not build".to_string(),
                "fix: give the bounds the same integer type, or use untyped constants (they adapt to the counter)".to_string(),
            ];
            self.diags.push(
                Diagnostic::error("HYB-E2009", self.path.clone(), span, msg).with_notes(notes),
            );
            return;
        }
        let counter = tl.unwrap_or_else(|| "int".to_string());
        let Some((clo, chi)) = crate::const_overflow::fixed_width_int_range(&counter) else {
            return;
        };
        for (bound, role) in [(lo, "lo"), (hi, "hi")] {
            let overflows = match crate::const_overflow::const_int_fold_with(
                bound,
                &self.const_values,
                0,
                Some(st),
            ) {
                crate::const_overflow::ConstFold::Val(v) => v < clo || v > chi,
                crate::const_overflow::ConstFold::TooBig => true,
                crate::const_overflow::ConstFold::NotConst => false,
            };
            if overflows {
                let span = bound.span();
                let msg = format!(
                    "constant bound `{}` ({role}) overflows the range counter's type `{counter}` — Go rejects `constant ... overflows {counter}` (HYB-E2009)",
                    bound.render()
                );
                let notes = vec![
                    "why: the counter takes `lo`'s type (defaulting to int) and Go folds constant bounds against it — an out-of-range constant is a compile error, so the emitted module would not build".to_string(),
                    "fix: use a constant within the counter's range, or widen the counter's type".to_string(),
                ];
                self.diags.push(
                    Diagnostic::error("HYB-E2009", self.path.clone(), span, msg).with_notes(notes),
                );
            }
        }
    }

    /// The bound's INTEGER type when a typed expression fixes it — `None`
    /// for untyped literals and untyped consts, which adapt to the
    /// counter's type in the lowering (and in Go's constant arithmetic).
    fn typed_int_bound(&self, st: &FnState, e: &Expr) -> Option<String> {
        match unwrap_parens(e) {
            Expr::Lit { .. } => None,
            Expr::Ident { name, .. } => {
                if st.env_lookup(name).is_none() {
                    if let Some(b) = self.consts.get(name.as_str()) {
                        // a file-level const: a TYPED const fixes the
                        // bound's type; an untyped const adapts.
                        let ty = b.ty.as_ref()?;
                        let resolved = self.resolve_type_alias(ty);
                        if let TypeExpr::Named { name: rn, .. } = resolved {
                            if crate::const_overflow::fixed_width_int_range(&rn).is_some() {
                                return Some(rn);
                            }
                        }
                        return None;
                    }
                }
                let ty = self.ty_of_expr(st, e)?;
                self.named_int_of(&ty)
            }
            Expr::Call { callee, args, .. } if args.len() == 1 && !args[0].spread => {
                // a fixed-width conversion names its own type: `u32(0)`
                // is a u32 bound regardless of what ty_of_expr infers.
                if let Expr::Ident { name, .. } = unwrap_parens(callee) {
                    if crate::const_overflow::fixed_width_int_range(name).is_some() {
                        return Some(name.clone());
                    }
                }
                let ty = self.ty_of_expr(st, e)?;
                self.named_int_of(&ty)
            }
            _ => {
                let ty = self.ty_of_expr(st, e)?;
                self.named_int_of(&ty)
            }
        }
    }

    fn named_int_of(&self, ty: &TypeExpr) -> Option<String> {
        let resolved = self.resolve_type_alias(ty);
        if let TypeExpr::Named { name, .. } = resolved {
            if crate::const_overflow::fixed_width_int_range(&name).is_some() {
                return Some(name);
            }
        }
        None
    }"""))

    # (registry) the corpus join keys on tier0_rule_id — declare the
    # invariant class this code enforces (BUILDABLE: every accepted
    # program must emit go-buildable Go; the issue bodies cite it).
    edits.append(("compiler/crates/hy-diag/data/diagnostics.json",
                  '"code": "HYB-E2009",',
                  '"code": "HYB-E2009",\n      "tier0_rule_id": "BUILDABLE",'))

    return edits
