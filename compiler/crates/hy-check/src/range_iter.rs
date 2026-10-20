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
                    if vars.len() > 1 && (is_chan || is_set) {
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
                kind: ForKind::Range { var, lo, hi, .. },
                ..
            } => {
                self.check_integer_range_bound(st, var, lo);
                self.check_integer_range_bound(st, var, hi);
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
            Expr::Unary { operand, .. } => {
                if let Some(k) = self.non_integer_bound_kind(st, operand) {
                    return Some(k);
                }
            }
            Expr::Binary { lhs, rhs, .. } => {
                if let Some(k) = self
                    .non_integer_bound_kind(st, lhs)
                    .or_else(|| self.non_integer_bound_kind(st, rhs))
                {
                    return Some(k);
                }
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
        let shape = self.shape_of_expr(st, expr);
        match shape {
            TypeShape::Primitive | TypeShape::Unknown => None,
            other => Some(other.render()),
        }
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
        let shape = match self.shape_of_expr(st, expr) {
            TypeShape::Pointer(inner) if matches!(*inner, TypeShape::Array(_)) => *inner,
            other => other,
        };
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
