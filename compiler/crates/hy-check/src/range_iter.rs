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
//!   * iterable kinds (Slice/Array/Str/Map/Chan/BorrowHandle) pass;
//!   * a concrete non-iterable is HYB-E2009;
//!   * an unresolvable operand stays fail-closed (HYB-E3005, O4.9) — the
//!     same treatment the send and receive sides give an unknown operand.
//!
//! `ForKind::Range` (`for i in 0..n`) has no iterable operand and is not
//! affected. Wiring: the Tier-0 flat walk (lib.rs, per flattened statement)
//! and the closure walk (closures.rs — FnLit bodies sit outside the flat
//! walk, the A5 perimeter).

use crate::model::{Checker, FnState, TypeShape};
use hy_diag::{Diagnostic, Span};
use hy_syntax::{Expr, ForKind, Stmt};

impl<'a> Checker<'a> {
    /// Statement entry: classify the iterable of every `for x in e` in this
    /// statement's own position. Nested block bodies are NOT recursed — the
    /// Tier-0 flat walk already lists those statements; the closure walk
    /// recurses explicitly.
    pub(crate) fn check_stmt_range_iterable(&mut self, st: &FnState, stmt: &Stmt) {
        if let Stmt::For { kind: ForKind::Iterate { expr, .. }, .. } = stmt {
            let span = expr.span();
            self.check_range_iterable(st, expr, span);
        }
    }

    /// Classify a range's ITERABLE — the mirror of `check_send_target`
    /// (issue #411): SPEC-LANGUAGE LX13 allows slices, arrays, strings,
    /// maps, and channels (plus L3 borrow views, which the shape layer
    /// already iterates — corpus 07/11/12). A concrete non-iterable is
    /// HYB-E2009; an unresolvable shape is fail-closed HYB-E3005 (O4.9),
    /// exactly as the send (#77) and receive (#409) sides treat an unknown
    /// operand. A TYPE contract, not a sendability doubt — `managed` does
    /// not suppress it.
    pub(crate) fn check_range_iterable(&mut self, st: &FnState, expr: &Expr, span: Span) -> bool {
        match self.shape_of_expr(st, expr) {
            TypeShape::Slice(_)
            | TypeShape::Array(_)
            | TypeShape::Str
            | TypeShape::Map(_, _)
            | TypeShape::Chan(_)
            | TypeShape::BorrowHandle(_) => true,
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
                false
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
                false
            }
        }
    }
}
