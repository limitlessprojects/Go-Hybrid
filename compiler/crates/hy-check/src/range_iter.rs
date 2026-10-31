//! range_iter.rs — `for x in e` iterable-kind checks (HYB-E2009 / E3005).
//!
//! Issue #411: SPEC-LANGUAGE LX13 pins the iterable set ("`for x in e`
//! iterates slices, arrays, strings (runes), maps (keys), channels (until
//! close), and integer ranges `for i in 0..n`"), but nothing checked that
//! the `e` of a `ForKind::Iterate` belongs to it. `for v in 5` / `for v in
//! true` / `for v in box{...}` / `for v in count()` (an int-returning call)
//! passed `hyb check` with 0 diagnostics and emitted `for _, v := range x`
//! — Go: `cannot range over x`. This mirrors the send-target (#77) and
//! receive-operand classification:
//!   * iterable kinds (Slice/Array/Str/Map/Chan/BorrowHandle, plus the
//!     element-source foreign handles) pass;
//!   * a concrete non-iterable is HYB-E2009;
//!   * an unresolvable operand stays fail-closed (HYB-E3005, O4.9) — the
//!     same treatment the send and receive sides give an unknown operand.
//!
//! Go's range rule also allows a pointer TO AN ARRAY (`*[N]T`), so exactly
//! one Pointer level is normalized for arrays — `*struct` / `*slice` /
//! `*string` stay non-iterable, as in Go.
//!
//! `ForKind::Range` (`for i in 0..n`) has no iterable operand and is not
//! affected. Wiring: the Tier-0 flat walk (lib.rs, per flattened statement)
//! and the closure walk (closures.rs — FnLit bodies sit outside the flat
//! walk, the A5 perimeter).

use crate::model::{unwrap_parens, Checker, FnState, TypeShape};
use hy_diag::{Diagnostic, Span};
use hy_syntax::{Expr, ForKind, LitKind, Stmt, TypeExpr};

impl<'a> Checker<'a> {
    /// Statement entry: classify the iterable of every `for x in e` and the
    /// integer bounds of every `for i in lo..hi` in this statement's own
    /// position. Nested block bodies are NOT recursed — the Tier-0 flat walk
    /// already lists those statements; the closure walk recurses explicitly.
    pub(crate) fn check_stmt_range_iterable(&mut self, st: &FnState, stmt: &Stmt) {
        match stmt {
            Stmt::For {
                kind: ForKind::Iterate { vars, expr },
                ..
            } => {
                let span = expr.span();
                let before = self.diags.len();
                self.check_range_iterable(st, expr, span);
                if self.diags.len() == before {
                    let shape = self.shape_of_expr(st, expr);
                    let is_chan = matches!(shape, TypeShape::Chan(_));
                    let is_set = matches!(
                        shape,
                        TypeShape::Struct(Some(ref n)) if n.starts_with("set[") || n == "set"
                    );
                    if vars.len() > 1 && (is_chan || is_set || shape.is_handle()) {
                        let kind_str = if is_chan { "channel" } else { "set" };
                        let why_note = if is_chan {
                            "why: Go permits only a single iteration variable when ranging over a channel (`range over … permits only one iteration variable`), so an accepted Hybrid program would emit unbuildable Go (LX13)".to_string()
                        } else {
                            "why: ranging over a set yields elements as single values; sets permit only one iteration variable (T9)".to_string()
                        };
                        let msg = format!(
                            "`for {}, {} in {}` binds {} variables, but ranging over a {} yields only 1 value (HYB-E2009)",
                            vars[0],
                            vars[1],
                            expr.render(),
                            vars.len(),
                            kind_str
                        );
                        let notes = vec![
                            why_note,
                            format!("fix: bind a single variable (`for {} in {} {{ … }}`)", vars[0], expr.render()),
                        ];
                        self.diags.push(
                            Diagnostic::error("HYB-E2009", self.path.clone(), span, msg)
                                .with_notes(notes),
                        );
                    } else if vars.len() > 2 {
                        let msg = format!(
                            "`for {} in {}` binds {} variables, but `for … in` permits at most 2 iteration variables (HYB-E2009)",
                            vars.join(", "),
                            expr.render(),
                            vars.len()
                        );
                        let notes = vec![
                            "why: Go's `range` clause permits at most two iteration variables (index/key and element), and the emitter cannot lower 3+ loop variables (LX13)".to_string(),
                            format!(
                                "fix: bind at most two variables (`for {}, {} in {} {{ … }}`)",
                                vars[0],
                                vars[1],
                                expr.render()
                            ),
                        ];
                        self.diags.push(
                            Diagnostic::error("HYB-E2009", self.path.clone(), span, msg)
                                .with_notes(notes),
                        );
                    }
                }
            }
            Stmt::For {
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
            }
            _ => {}
        }
    }

    fn check_integer_range_bound(&mut self, st: &FnState, var: &str, bound: &Expr) {
        let non_int_desc = self.non_integer_bound_kind(st, bound);
        let Some(kind_desc) = non_int_desc else {
            return;
        };
        let span = bound.span();
        let msg = format!(
            "`{}` ({kind_desc}) is not an integer — `for {var} in lo..hi` requires integer range bounds (LX13) (HYB-E2009)",
            bound.render()
        );
        let notes = vec![
            "why: SPEC-LANGUAGE LX13 defines `for i in lo..hi` as an integer range lowered to `for i := lo; i < hi; i++`, which Go rejects on non-integer bounds".to_string(),
            "fix: use integer expressions for both `lo` and `hi` (e.g. `for i in 0..len(xs)`), or iterate a collection directly with `for x in xs`".to_string(),
        ];
        self.diags.push(
            Diagnostic::error("HYB-E2009", self.path.clone(), span, msg).with_notes(notes),
        );
    }

    fn non_integer_bound_kind(&self, st: &FnState, expr: &Expr) -> Option<String> {
        self.non_integer_bound_kind_at(st, expr, 0)
    }

    /// Issue #1554: file `const` initializers can be compound expressions
    /// (`const hi = 1.5 + 0.5`, `const flag = !false`). Resolve them through
    /// the same classifier instead of only classifying bare literals; the
    /// depth cap keeps a `const a = b; const b = a` cycle finite.
    fn non_integer_bound_kind_at(
        &self,
        st: &FnState,
        expr: &Expr,
        depth: usize,
    ) -> Option<String> {
        if depth > 32 {
            return None;
        }
        match unwrap_parens(expr) {
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
            }
            Expr::Unary { op, operand, .. } => {
                // Issue #827: `!x` yields a bool — never an integer bound.
                if matches!(op, hy_syntax::UnOp::Not) {
                    return Some("bool".to_string());
                }
                if let Some(k) = self.non_integer_bound_kind_at(st, operand, depth + 1) {
                    return Some(k);
                }
            }
            Expr::Binary { op, lhs, rhs, .. } => {
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
                    .non_integer_bound_kind_at(st, lhs, depth + 1)
                    .or_else(|| self.non_integer_bound_kind_at(st, rhs, depth + 1))
                {
                    return Some(k);
                }
            }
            Expr::AddrOf { .. } => {
                // Issue #827: `&x` is a pointer — never an integer bound.
                return Some("pointer".to_string());
            }
            _ => {}
        }
        if let Some(ty) = self.ty_of_expr(st, expr) {
            let resolved = self.resolve_type_alias(&ty);
            if let TypeExpr::Named { name, .. } = &resolved {
                if matches!(
                    name.as_str(),
                    "bool"
                        | "float32"
                        | "float64"
                        | "f32"
                        | "f64"
                        | "complex64"
                        | "complex128"
                        | "string"
                        | "error"
                ) {
                    return Some(resolved.render());
                }
            }
        }
        // Issue #827: an untyped const identifier carries no type
        // ascription, so `ty_of_expr`/`shape_of_expr` see nothing. Resolve
        // the initializer through the file const map (#998 pattern) and
        // classify the literal. Block-scoped consts stay fail-open (no
        // const-value map in FnState — the #998 precedent).
        if let Expr::Ident { name, .. } = unwrap_parens(expr) {
            if st.env_lookup(name).is_none() {
                if let Some(init) = self.const_values.get(name.as_str()) {
                    if let Some(k) = self.non_integer_bound_kind_at(st, init, depth + 1) {
                        return Some(k);
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
        // Issue #1165: compare the two bounds as *Go types*, not as spellings.
        // `byte` and `rune` are Go aliases of `uint8`/`int32`, and `u8`/`i32`
        // are the SPEC spellings the emitter maps onto those same Go types, so
        // `var a u8 = 0; var b uint8 = 10; for i in a..b` is a legal loop that
        // was rejected with a false HYB-E2009. Canonicalise before comparing;
        // the diagnostic still prints the spellings the author wrote.
        let tl_c = tl.as_deref().map(crate::const_overflow::canonical_int_type_name);
        let th_c = th.as_deref().map(crate::const_overflow::canonical_int_type_name);
        let mismatch = match (&tl_c, &th_c) {
            (Some(a), Some(b)) => a != b,
            (None, Some(b)) => *b != "int",
            (Some(a), None) => inclusive && *a != "int",
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
    }

    /// Classify a range's ITERABLE — the mirror of `check_send_target`
    /// (issue #411): SPEC-LANGUAGE LX13 allows slices, arrays, strings,
    /// maps, and channels (plus L3 borrow views, which the shape layer
    /// already iterates — corpus 05/07/11/12, and the foreign element-source
    /// handles the emitter ranges via its ForeignTy branch). A concrete
    /// non-iterable is HYB-E2009; an unresolvable shape is fail-closed
    /// HYB-E3005 (O4.9), exactly as the send (#77) and receive (#409)
    /// sides treat an unknown operand. A TYPE contract, not a sendability
    /// doubt — `managed` does not suppress it.
    pub(crate) fn check_range_iterable(&mut self, st: &FnState, expr: &Expr, span: Span) {
        // Go: "array, pointer to array, slice, string, map, or channel" —
        // only pointer-to-ARRAY is rangeable (`*struct` / `*slice` /
        // `*string` are not), so normalize exactly one Pointer level for
        // arrays and leave every other shape as classified.
        let shape = if matches!(self.ty_of_expr(st, expr).map(|t| self.resolve_type_alias(&t)), Some(TypeExpr::Option { .. })) { TypeShape::Struct(None) } else { match self.shape_of_expr(st, expr) {
            TypeShape::Pointer(inner) if matches!(*inner, TypeShape::Array(_)) => *inner,
            other => other,
        } };
        match shape {
            TypeShape::Slice(_)
            | TypeShape::Array(_)
            | TypeShape::Str
            | TypeShape::Map(_, _)
            // The L3 borrow views (e.g. `doc.array("users")?` — the element
            // view the emitter iterates; corpus 05/07/11/12). Corpus-driven,
            // NOT a Go range shape — Go cannot range the view struct itself.
            | TypeShape::BorrowHandle(_) => {}
            TypeShape::Struct(Some(ref n)) if n.starts_with("set[") || n == "set" => {}
            TypeShape::Chan(_) => {
                let chan_ty = self.ty_of_expr(st, expr).or_else(|| {
                    if let Expr::Ident { name, .. } = unwrap_parens(expr) {
                        let root = crate::freeze::resolve_alias(st, name);
                        self.lookup_binding(st, &root).and_then(|b| b.ty.clone())
                    } else {
                        None
                    }
                });
                if let Some(ty) = chan_ty {
                    let resolved = self.resolve_type_alias(&ty);
                    if matches!(
                        resolved,
                        hy_syntax::TypeExpr::Chan {
                            dir: hy_syntax::ChanDir::Send,
                            ..
                        }
                    ) {
                        let msg = format!(
                            "cannot range over `{}` (`{}`) — channel is send-only (HYB-E2009)",
                            expr.render(),
                            resolved.render()
                        );
                        let notes = vec![
                            "why: Go rejects `range` over a `chan<- T` send-only channel (`invalid operation: cannot range over … (receive from send-only type)`), so an accepted Hybrid program would emit unbuildable Go".to_string(),
                            "fix: declare the parameter or binding as a bidirectional (`chan T`) or receive-only (`<-chan T`) channel".to_string(),
                        ];
                        self.diags.push(
                            Diagnostic::error("HYB-E2009", self.path.clone(), span, msg)
                                .with_notes(notes),
                        );
                    }
                }
            }
            // The foreign handles the emitter ranges as element sources
            // (the ForeignTy range branch); `serde_json.value` only —
            // other handles (`flate2.writer`) are not iterable and fall
            // through to the concrete-non-iterable arm.
            TypeShape::OwnHandle(ref t) if crate::foreign::is_iterable_handle(t) => {}
            TypeShape::Unknown => {
                let msg = format!(
                    "cannot prove `{}` is iterable — `for x in e` needs a slice, array, string, map, or channel (LX13; O4.9)",
                    expr.render()
                );
                let notes = vec![
                    "why: the checker cannot classify the iterable's type, and Go cannot range over it (issue #411)".to_string(),
                    "fix: bind the iterable to a local with a visible type first, so the checker can resolve it".to_string(),
                ];
                self.diags.push(
                    Diagnostic::error("HYB-E3005", self.path.clone(), span, msg).with_notes(notes),
                );
            }
            other => {
                let msg = format!(
                    "`{}` ({}) is not iterable — `for x in e` iterates slices, arrays, strings, maps, and channels (LX13)",
                    expr.render(),
                    other.render()
                );
                let notes = vec![
                    "why: ranging over a non-iterable is not valid Go (`cannot range over`); the accepted subset only emits buildable Go (issue #411)".to_string(),
                    "fix: iterate a slice/array/string/map/chan binding, or use the integer-range form `for i in 0..n`".to_string(),
                ];
                self.diags.push(
                    Diagnostic::error("HYB-E2009", self.path.clone(), span, msg).with_notes(notes),
                );
            }
        }
    }
}
