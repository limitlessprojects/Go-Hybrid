#!/usr/bin/env python3
"""sp_tmpl_e6006.py — T-E6006 template family (bake-off arm A0, issue #823).

CLASS-LEVEL transplants only:
  * paren blindspot removal — the #928-era `unwrap_parens` discipline
  * target-set completion (`bool`) + the identity rule `bool(true)`
  * alias resolution via the EXISTING `resolve_type_alias` machinery
  * #998 pattern — `conv_const` becomes const-map-aware (`conv_const_with`)
    and folds constant binary/unary operands (int via the shared `fold_bin`,
    float via f64, string concat, comparisons to bool constants)
  * #999 pattern — wire the EXISTING `check_expr_type_conversions` predicate
    onto the uncovered file-level `Decl::Const` initializer site (gap 5)
  * #725-derived discipline — the emitter's SPEC X4 fold rewrites LITERAL
    operands only, so the sub-64 skip applies to literals; a constant folded
    from a named const stays an identifier in the emitted Go and must pass
    the strict range check (Go rejects the overflow).

Completion contract = all five families the issue enumerates.
Block-scoped consts stay fail-open (the #998 precedent), disclosed.
"""

from __future__ import annotations

CO = "compiler/crates/hy-check/src/const_overflow.rs"
LIB = "compiler/crates/hy-check/src/lib.rs"

TEST_PATH = "compiler/crates/hy-check/tests/const_conversion_target_forms.rs"
TEST_STEM = "const_conversion_target_forms"

TEST_SRC = '''//! #823 — invalid constant conversions in disguise (HYB-E6006).
//!
//! Go rejects `(int)("abc")`, `bool(1)`, `my_int("abc")` (a type alias),
//! `int(f)` on `const f = 2.5`, and file-level `const a = int(2.5)` at
//! compile time. `hyb check` accepted all of them: the conversion guard
//! matched the callee without unwrapping parens, omitted `bool` from the
//! target set, never resolved `type_aliases`, folded only bare literal
//! operands, and the file-const pass never invoked the conversion check.
//! Runtime conversions and the SPEC X4 literal fold (`u8(300) == 44`)
//! stay accepted.

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
fn parenthesized_conversion_callees_reject() {
    assert_code(
        "fn main() {\\n    x := (int)(\\"abc\\")\\n    _ = x\\n}\\n",
        "HYB-E6006",
    );
    assert_code(
        "fn main() {\\n    x := (int8)(2.5)\\n    _ = x\\n}\\n",
        "HYB-E6006",
    );
    assert_code(
        "fn main() {\\n    x := ([]int)(\\"abc\\")\\n    _ = x\\n}\\n",
        "HYB-E6006",
    );
}

#[test]
fn bool_conversion_target_rejects_non_bool() {
    assert_code(
        "fn main() {\\n    x := bool(1)\\n    _ = x\\n}\\n",
        "HYB-E6006",
    );
    assert_code(
        "fn main() {\\n    x := bool(2.5)\\n    _ = x\\n}\\n",
        "HYB-E6006",
    );
    assert_code(
        "fn main() {\\n    x := bool(\\"abc\\")\\n    _ = x\\n}\\n",
        "HYB-E6006",
    );
}

#[test]
fn type_aliased_targets_reject() {
    assert_code(
        "type my_int = int\\n\\nfn main() {\\n    x := my_int(\\"abc\\")\\n    _ = x\\n}\\n",
        "HYB-E6006",
    );
    assert_code(
        "type my_u8 = u8\\nconst n = 300\\n\\nfn main() {\\n    x := my_u8(n)\\n    _ = x\\n}\\n",
        "HYB-E6006",
    );
    assert_code(
        "type my_slice = []int\\n\\nfn main() {\\n    x := my_slice(\\"abc\\")\\n    _ = x\\n}\\n",
        "HYB-E6006",
    );
    assert_code(
        "type my_elem = int\\n\\nfn main() {\\n    x := []my_elem(\\"abc\\")\\n    _ = x\\n}\\n",
        "HYB-E6006",
    );
}

#[test]
fn const_ident_and_folded_operands_reject() {
    assert_code(
        "const f = 2.5\\n\\nfn main() {\\n    x := int(f)\\n    _ = x\\n}\\n",
        "HYB-E6006",
    );
    assert_code(
        "fn main() {\\n    x := int(1.5 + 1.0)\\n    _ = x\\n}\\n",
        "HYB-E6006",
    );
    assert_code(
        "fn main() {\\n    x := int(1 < 2)\\n    _ = x\\n}\\n",
        "HYB-E6006",
    );
    assert_code(
        "const s = \\"abc\\"\\n\\nfn main() {\\n    x := []int(s)\\n    _ = x\\n}\\n",
        "HYB-E6006",
    );
    assert_code(
        "const n = 300\\n\\nfn main() {\\n    x := u8(n)\\n    _ = x\\n}\\n",
        "HYB-E6006",
    );
    assert_code(
        "fn main() {\\n    x := i64(1 << 63)\\n    _ = x\\n}\\n",
        "HYB-E6006",
    );
    assert_code(
        "fn main() {\\n    x := u64(1 << 64)\\n    _ = x\\n}\\n",
        "HYB-E6006",
    );
    assert_code(
        "fn main() {\\n    x := f32(1e30 * 1e20)\\n    _ = x\\n}\\n",
        "HYB-E6006",
    );
}

#[test]
fn file_const_initializer_conversions_reject() {
    assert_code(
        "const a = int(2.5)\\n\\nfn main() {\\n    _ = a\\n}\\n",
        "HYB-E6006",
    );
    assert_code(
        "const d = bool(1)\\n\\nfn main() {\\n    _ = d\\n}\\n",
        "HYB-E6006",
    );
}

#[test]
fn valid_conversions_stay_legal() {
    // identity bool conversion
    assert_ok("fn main() {\\n    x := bool(true)\\n    _ = x\\n}\\n");
    // representable float constant to int
    assert_ok("fn main() {\\n    x := int(3.0)\\n    _ = x\\n}\\n");
    // runtime conversion of a variable
    assert_ok("fn main() {\\n    f := 3\\n    x := int(f)\\n    _ = x\\n}\\n");
    // SPEC X4 literal fold: u8(300) == 44 (sub-64 skip is literal-only)
    assert_ok("fn main() {\\n    x := u8(300)\\n    _ = x\\n}\\n");
    // type name as conversion callee with a legal operand
    assert_ok("fn main() {\\n    x := int(2)\\n    _ = x\\n}\\n");
    // string conversion of an integer constant
    assert_ok("fn main() {\\n    x := string(65)\\n    _ = x\\n}\\n");
}
'''

FIXTURE_PATH = "tests/checker-corpus/const-conversion-target-forms-reject.hy"
FIXTURE_SRC = '''// ============================================================================
// const-conversion-target-forms-reject — Checker corpus (issue #823).
//
// PURPOSE: invalid constant conversions in disguise — parenthesized callees,
// `bool(const)`, type-aliased targets, const-identifier/folded operands,
// and file-level const initializers — not just the bare `Type(literal)`
// shape the #228 rule already caught.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — every `hyb check`-
// accepted file must build cleanly with `go build`. Go refuses all of
// these at compile time (`cannot convert ... (untyped ... constant) to
// type ...`).
//
// ---- Why this is a REJECT ----
// The conversion guard matched the callee without `unwrap_parens`, omitted
// `bool` from the target set, never resolved `self.type_aliases`, folded
// only bare literal operands (`conv_const`), and the file-const pass never
// invoked the conversion check. The fix mirrors each gap with existing
// machinery (resolve_type_alias, the #998 const map, the #725 fold) and
// wires the existing predicate onto the file-const site (#999 pattern).
// The SPEC X4 literal fold (`u8(300) == 44`) stays accepted — but only
// for LITERAL operands, which is what the emitter actually rewrites.
//
// machine-readable verdict marker below; regression pins live in
// `hy-check/tests/const_conversion_target_forms.rs`.
// ============================================================================

type my_int = int
const f = 2.5
const a = int(2.5)

fn main() {
    x1 := (int)("abc")
    x2 := bool(1)
    x3 := my_int("abc")
    x5 := int(f)
    _ = x1
    _ = x2
    _ = x3
    _ = x5
    _ = a
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
                "T-E6006 full: unwrap_parens on callees (parens blindspot), "
                "bool target + identity rule, type-alias resolution via the "
                "existing resolve_type_alias, conv_const_with (the #998 "
                "const map + fold_bin arithmetic/comparison folds), the "
                "literal-only X4 skip discipline (#725 emitter behavior), "
                "and the file-const pass wiring (#999 pattern)."
            ),
        },
    ]


def _edits():
    edits = []

    # (1) slice-conversion callee branch: unwrap parens, bind the bare form.
    edits.append((CO,
        """        if let Expr::SliceLit {
            elem: Some(t),
            items,
            ..
        } = callee
        {
            if items.is_empty() && callee.as_array_lit().is_none() {""",
        """        let bare_callee = unwrap_parens(callee);
        if let Expr::SliceLit {
            elem: Some(t),
            items,
            ..
        } = bare_callee
        {
            if items.is_empty() && bare_callee.as_array_lit().is_none() {"""))

    # (2) the conversion-call tail: parens + alias resolution + bool target
    # + const-map-aware operand fold.
    edits.append((CO,
        """        let Expr::Ident { name, .. } = callee else {
            return;
        };
        if !is_conversion_target(name) {
            return;
        }
        let Some(c) = conv_const(arg) else {
            return; // non-constant operand: run-time conversion (SPEC X4)
        };
        self.check_conv_against_target(name, &c, arg, span);""",
        """        let Expr::Ident { name, .. } = unwrap_parens(callee) else {
            return;
        };
        // Issue #823: type-aliased targets convert exactly like their
        // resolution — `my_int("abc")` is `int("abc")`; a slice alias
        // routes to the slice-conversion check with the resolved element.
        let resolved = self.resolve_type_alias(&TypeExpr::Named {
            name: name.clone(),
            span: callee.span(),
        });
        let target_name: String = match &resolved {
            TypeExpr::Named { name: rn, .. } => rn.clone(),
            TypeExpr::Slice { elem: Some(el), .. } => {
                self.check_slice_conversion(el, arg, span);
                return;
            }
            _ => return,
        };
        if !is_conversion_target(&target_name) {
            return;
        }
        let Some(c) = conv_const_with(arg, &self.const_values, 0) else {
            return; // non-constant operand: run-time conversion (SPEC X4)
        };
        self.check_conv_against_target(&target_name, &c, arg, span);"""))

    # (3) check_slice_conversion: resolve the ELEMENT alias too
    # (`[]my_elem("abc")`).
    edits.append((CO,
        """    fn check_slice_conversion(&mut self, elem: &TypeExpr, arg: &Expr, span: Span) {
        let TypeExpr::Named { name: slice_elem, .. } = elem else {
            return; // []T with a non-named element + a run-time operand
        };""",
        """    fn check_slice_conversion(&mut self, elem: &TypeExpr, arg: &Expr, span: Span) {
        // Issue #823: resolve type aliases on the element — `[]my_elem`
        // converts exactly like its resolution.
        let resolved_elem = self.resolve_type_alias(elem);
        let TypeExpr::Named { name: slice_elem, .. } = &resolved_elem else {
            return; // []T with a non-named element + a run-time operand
        };"""))

    # (4) the bool target + the literal-only X4 skip discipline.
    edits.append((CO,
        """/// True when `name` is a built-in conversion target of `Type(const)` calls.
fn is_conversion_target(name: &str) -> bool {
    fixed_width_int_range(name).is_some()
        || matches!(name, "f32" | "f64" | "float32" | "float64" | "string")
}""",
        """/// True when `name` is a built-in conversion target of `Type(const)` calls.
/// Issue #823: `bool` joins — the emitter emits `bool(...)` and Go accepts
/// only the identity `bool(true)`.
fn is_conversion_target(name: &str) -> bool {
    fixed_width_int_range(name).is_some()
        || matches!(name, "f32" | "f64" | "float32" | "float64" | "string" | "bool")
}"""))

    # (5) the bool arm at the head of check_conv_against_target.
    edits.append((CO,
        """    fn check_conv_against_target(&mut self, target: &str, c: &ConvConst, arg: &Expr, span: Span) {
        if let Some((lo, hi)) = fixed_width_int_range(target) {""",
        """    fn check_conv_against_target(&mut self, target: &str, c: &ConvConst, arg: &Expr, span: Span) {
        // Issue #823: Go accepts only the identity `bool(true)`; every
        // other constant operand is `cannot convert ... to type bool`.
        if target == "bool" {
            if !matches!(c, ConvConst::Bool) {
                self.emit_conv_kind_error(target, c, arg, span);
            }
            return;
        }
        if let Some((lo, hi)) = fixed_width_int_range(target) {"""))

    # (6) the literal-only X4 skip: a constant folded from a NAMED CONST
    # stays an identifier in the emitted Go — Go rejects its overflow.
    edits.append((CO,
        """                ConvConst::Int(v) => {
                    if *v < lo || *v > hi {
                        if is_64bit_int_target(target) {""",
        """                ConvConst::Int(v) => {
                    if *v < lo || *v > hi {
                        // Issue #823: the emitter's SPEC X4 fold rewrites
                        // LITERAL operands only (`u8(300)` -> `uint8(44)`).
                        // A constant folded from a named const stays an
                        // identifier in the emitted Go, where Go itself
                        // rejects the overflow — only literals enjoy the
                        // sub-64 skip.
                        let literal_operand =
                            matches!(unwrap_parens(arg), Expr::Lit { .. });
                        if is_64bit_int_target(target) || !literal_operand {"""))

    # (7) conv_const_with — the #998 const-map-aware operand fold.
    edits.append((CO,
        """fn conv_const(e: &Expr) -> Option<ConvConst> {""",
        '''/// Issue #823: `conv_const` made const-map-aware — file-level `const`
/// identifiers resolve to their initializers (#998 pattern) and constant
/// binary expressions fold: int arithmetic via the shared `fold_bin`
/// (#725 machinery), float via f64, string concat, comparisons to untyped
/// bool constants (Go folds `1 < 2` at compile time). Block-scoped consts
/// stay fail-open (FnState carries no const-value map — #998 precedent).
fn conv_const_with(
    e: &Expr,
    consts: &std::collections::HashMap<String, &Expr>,
    depth: u32,
) -> Option<ConvConst> {
    if depth > 16 {
        return None;
    }
    let bare = unwrap_parens(e);
    match bare {
        Expr::Ident { name, .. } => {
            let v = consts.get(name.as_str())?;
            conv_const_with(v, consts, depth + 1)
        }
        Expr::Binary { op, lhs, rhs, .. } => {
            let l = conv_const_with(lhs, consts, depth + 1)?;
            let r = conv_const_with(rhs, consts, depth + 1)?;
            if matches!(
                op,
                BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge | BinOp::Eq | BinOp::Ne
            ) {
                // a comparison of constants is an untyped bool constant;
                // the VALUE never matters — targets reject the Bool kind
                // (identity on `bool` was decided before the fold).
                let _ = (conv_as_float(&l), conv_as_float(&r));
                return Some(ConvConst::Bool);
            }
            match (l, r) {
                (ConvConst::Str, ConvConst::Str) if matches!(op, BinOp::Add) => {
                    Some(ConvConst::Str)
                }
                (ConvConst::Bool, ConvConst::Bool)
                    if matches!(op, BinOp::And | BinOp::Or) =>
                {
                    Some(ConvConst::Bool)
                }
                (a, b) => {
                    let fa = conv_as_float(&a)?;
                    let fb = conv_as_float(&b)?;
                    match op {
                        BinOp::Add => Some(ConvConst::Float(fa + fb)),
                        BinOp::Sub => Some(ConvConst::Float(fa - fb)),
                        BinOp::Mul => Some(ConvConst::Float(fa * fb)),
                        BinOp::Div => Some(ConvConst::Float(fa / fb)),
                        _ => None,
                    }
                }
            }
        }
        _ => conv_const(bare),
    }
}

/// Best f64 reading of a folded constant for float arithmetic and
/// comparisons; non-numeric kinds (Str/Bool/Nil) are None.
fn conv_as_float(c: &ConvConst) -> Option<f64> {
    match c {
        ConvConst::Int(v) => Some(*v as f64),
        ConvConst::Float(f) => Some(*f),
        ConvConst::Char(v) => Some(*v as f64),
        ConvConst::Huge { f, .. } => Some(*f),
        ConvConst::Str | ConvConst::Bool | ConvConst::Nil => None,
    }
}

fn conv_const(e: &Expr) -> Option<ConvConst> {'''))

    # (8) the slice-conversion operand fold becomes map-aware too.
    edits.append((CO,
        "        match conv_const(arg) {",
        "        match conv_const_with(arg, &self.const_values, 0) {"))

    # (9) gap 5 — the file-const pass wires the EXISTING conversion
    # predicate onto Decl::Const initializers (#999 pattern).
    edits.append((LIB,
        """                ck.check_expr_emit_guard(&FnState::default(), &c.value);
                ck.check_composite_nils(&FnState::default(), &c.value);
                ck.check_expr_builtin_args(&c.value);""",
        """                ck.check_expr_emit_guard(&FnState::default(), &c.value);
                ck.check_composite_nils(&FnState::default(), &c.value);
                ck.check_expr_builtin_args(&c.value);
                // Issue #823 gap 5: file-level const initializers go through
                // the SAME conversion guard as statement expressions —
                // `const a = int(2.5)` must reject like `x := int(2.5)`.
                ck.check_expr_type_conversions(&c.value);"""))

    # (registry) the corpus join keys on tier0_rule_id — declare the
    # invariant class this code enforces (BUILDABLE: every accepted
    # program must emit go-buildable Go; the issue bodies cite it).
    edits.append(("compiler/crates/hy-diag/data/diagnostics.json",
                  '"code": "HYB-E6006",',
                  '"code": "HYB-E6006",\n      "tier0_rule_id": "BUILDABLE",'))

    return edits
