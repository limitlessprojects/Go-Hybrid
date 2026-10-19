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
                kind:
                    ForKind::Range {
                        var,
                        lo,
                        hi,
                        inclusive,
                    },
                ..
            } => {
                let before = self.diags.len();
                self.check_integer_range_bound(st, var, lo);
                self.check_integer_range_bound(st, var, hi);
                if self.diags.len() == before {
                    self.check_integer_range_compatibility_and_overflow(
                        st, var, lo, hi, *inclusive,
                    );
                }
            }
            _ => {}
        }
    }

    fn check_integer_range_bound(&mut self, st: &FnState, var: &str, bound: &Expr) {
        let non_int_desc = self.non_integer_bound_kind(st, bound, 0);
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

    fn non_integer_bound_kind(&self, st: &FnState, expr: &Expr, depth: u32) -> Option<String> {
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
                kind: LitKind::Nil, ..
            } => return Some("nil".to_string()),
            Expr::Lit {
                kind: LitKind::Str(_) | LitKind::RawStr(_),
                ..
            }
            | Expr::Interp { .. } => return Some("string".to_string()),
            Expr::AddrOf { .. } => return Some("pointer".to_string()),
            Expr::Ident { name, span } => {
                if name == "nil" {
                    return Some("nil".to_string());
                }
                if name == "true" || name == "false" {
                    return Some("bool".to_string());
                }
                match self.resolve_scoped_const_in_range(name, span.start) {
                    RangeScopedBinding::ShadowedByRuntimeVar => {}
                    RangeScopedBinding::LocalConst { ty, val } => {
                        if let Some(t) = ty {
                            if let Some(k) = self.non_integer_type_expr_kind(t) {
                                return Some(k);
                            }
                        }
                        if let Some(k) = self.non_integer_bound_kind(st, val, depth + 1) {
                            return Some(k);
                        }
                    }
                    RangeScopedBinding::None => {
                        if st.env_lookup(name).is_none() {
                            if let Some(b) = self.consts.get(name.as_str()) {
                                if let Some(t) = &b.ty {
                                    if let Some(k) = self.non_integer_type_expr_kind(t) {
                                        return Some(k);
                                    }
                                }
                            }
                            if let Some(val) = self.const_values.get(name.as_str()) {
                                if let Some(k) = self.non_integer_bound_kind(st, val, depth + 1) {
                                    return Some(k);
                                }
                            }
                        }
                    }
                }
            }
            Expr::Unary { op, operand, .. } => match op {
                hy_syntax::UnOp::Not => return Some("bool".to_string()),
                hy_syntax::UnOp::Addr => return Some("pointer".to_string()),
                hy_syntax::UnOp::Neg => {
                    if let Some(k) = self.non_integer_bound_kind(st, operand, depth + 1) {
                        return Some(k);
                    }
                }
            },
            Expr::Binary { op, lhs, rhs, .. } => {
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
                    .non_integer_bound_kind(st, lhs, depth + 1)
                    .or_else(|| self.non_integer_bound_kind(st, rhs, depth + 1))
                {
                    return Some(k);
                }
            }
            _ => {}
        }
        if let Some(ty) = self.ty_of_expr(st, expr) {
            if let Some(k) = self.non_integer_type_expr_kind(&ty) {
                return Some(k);
            }
        }
        let shape = self.shape_of_expr(st, expr);
        match shape {
            TypeShape::Primitive | TypeShape::Unknown => None,
            other => Some(other.render()),
        }
    }

    fn non_integer_type_expr_kind(&self, ty: &TypeExpr) -> Option<String> {
        let resolved = self.resolve_type_alias(ty);
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
        None
    }

    fn check_integer_range_compatibility_and_overflow(
        &mut self,
        st: &FnState,
        var: &str,
        lo: &Expr,
        hi: &Expr,
        inclusive: bool,
    ) {
        let lo_ty = self.concrete_int_bound_type(st, lo, 0);
        let hi_ty = self.concrete_int_bound_type(st, hi, 0);
        match (lo_ty, hi_ty) {
            (Some(tl), Some(tr)) if tl != tr => {
                let msg = format!(
                    "mismatched integer range bounds `{}` (`{tl}`) and `{}` (`{tr}`) in `for {var} in …` (LX13) (HYB-E2009)",
                    lo.render(),
                    hi.render()
                );
                let notes = vec![
                    format!(
                        "why: `for {var} in lo..hi` lowers to `for {var} := lo; {var} < hi; {var}++`, and Go rejects comparing `{tl}` with `{tr}` (`mismatched types {tl} and {tr}`)"
                    ),
                    format!(
                        "fix: convert both bounds to the same integer type (e.g. `int({})` or `{tl}({})`)",
                        hi.render(),
                        hi.render()
                    ),
                ];
                self.diags.push(
                    Diagnostic::error("HYB-E2009", self.path.clone(), hi.span(), msg)
                        .with_notes(notes),
                );
                return;
            }
            (None, Some(tr)) if tr != "int" => {
                let msg = format!(
                    "range bound `{}` has type `{tr}`, but untyped lower bound `{}` defaults loop counter `{var}` to `int` (LX13) (HYB-E2009)",
                    hi.render(),
                    lo.render()
                );
                let notes = vec![
                    format!(
                        "why: `for {var} := {}; {var} < {}; {var}++` gives `{var}` Go's default untyped integer type `int`, which cannot be compared against `{tr}` (`mismatched types int and {tr}`)",
                        lo.render(),
                        hi.render()
                    ),
                    format!(
                        "fix: convert the upper bound to `int` (`for {var} in {}..int({})`) or ascribe the lower bound (`{tr}({})..{}`)",
                        lo.render(),
                        hi.render(),
                        lo.render(),
                        hi.render()
                    ),
                ];
                self.diags.push(
                    Diagnostic::error("HYB-E2009", self.path.clone(), hi.span(), msg)
                        .with_notes(notes),
                );
                return;
            }
            (Some(tl), None) if inclusive && tl != "int" => {
                let msg = format!(
                    "inclusive range lower bound `{}` has type `{tl}`, but untyped upper bound `{}` defaults to `int` (LX13) (HYB-E2009)",
                    lo.render(),
                    hi.render()
                );
                let notes = vec![
                    format!(
                        "why: inclusive `for {var} in lo..=hi` binds both bounds before comparing `hyLo <= hyHi`, so an untyped `hi` defaults to `int` and fails `go build` (`mismatched types {tl} and int`)"
                    ),
                    format!(
                        "fix: convert the upper bound to `{tl}` (`for {var} in {}..={tl}({})`) or convert the lower bound to `int`",
                        lo.render(),
                        hi.render()
                    ),
                ];
                self.diags.push(
                    Diagnostic::error("HYB-E2009", self.path.clone(), hi.span(), msg)
                        .with_notes(notes),
                );
                return;
            }
            _ => {}
        }

        let target_int_ty = lo_ty.or(hi_ty).unwrap_or("int");
        if let Some((min_v, max_v)) = crate::const_overflow::fixed_width_int_range(target_int_ty) {
            for bound in [lo, hi] {
                if self.concrete_int_bound_type(st, bound, 0).is_none()
                    && self.const_bound_overflows_range(st, bound, min_v, max_v)
                {
                    let msg = format!(
                        "constant range bound `{}` overflows loop counter type `{target_int_ty}` in `for {var} in …` (LX13) (HYB-E2009)",
                        bound.render()
                    );
                    let notes = vec![
                        format!(
                            "why: Go rejects `{}` when assigned to or compared against `{target_int_ty}` (`overflows {target_int_ty}`), so the emitted loop would fail `go build`",
                            bound.render()
                        ),
                        format!(
                            "fix: use a bound within [{min_v}, {max_v}] or widen the loop counter type"
                        ),
                    ];
                    self.diags.push(
                        Diagnostic::error("HYB-E2009", self.path.clone(), bound.span(), msg)
                            .with_notes(notes),
                    );
                }
            }
        }
    }

    fn concrete_int_bound_type(
        &self,
        st: &FnState,
        expr: &Expr,
        depth: u32,
    ) -> Option<&'static str> {
        if depth > 32 {
            return None;
        }
        let bare = unwrap_parens(expr);
        match bare {
            Expr::Call { callee, args, .. } => {
                if let Expr::Ident { name, .. } = unwrap_parens(callee) {
                    if (name == "len" || name == "cap") && args.len() == 1 {
                        return Some("int");
                    }
                }
            }
            Expr::Ident { name, span } => {
                match self.resolve_scoped_const_in_range(name, span.start) {
                    RangeScopedBinding::ShadowedByRuntimeVar => {}
                    RangeScopedBinding::LocalConst { ty, val } => {
                        if let Some(t) = ty {
                            return self.canonical_go_int_type(t);
                        }
                        return self.concrete_int_bound_type(st, val, depth + 1);
                    }
                    RangeScopedBinding::None => {
                        if st.env_lookup(name).is_none() {
                            if let Some(b) = self.consts.get(name.as_str()) {
                                if let Some(t) = &b.ty {
                                    return self.canonical_go_int_type(t);
                                }
                            }
                            if let Some(val) = self.const_values.get(name.as_str()) {
                                return self.concrete_int_bound_type(st, val, depth + 1);
                            }
                        }
                    }
                }
            }
            Expr::Unary {
                op: hy_syntax::UnOp::Neg,
                operand,
                ..
            } => return self.concrete_int_bound_type(st, operand, depth + 1),
            Expr::Binary { op, lhs, rhs, .. } => match op {
                hy_syntax::BinOp::Shl | hy_syntax::BinOp::Shr => {
                    return self.concrete_int_bound_type(st, lhs, depth + 1);
                }
                hy_syntax::BinOp::Add
                | hy_syntax::BinOp::Sub
                | hy_syntax::BinOp::Mul
                | hy_syntax::BinOp::Div
                | hy_syntax::BinOp::Mod
                | hy_syntax::BinOp::BitAnd
                | hy_syntax::BinOp::BitOr
                | hy_syntax::BinOp::BitXor => {
                    return self
                        .concrete_int_bound_type(st, lhs, depth + 1)
                        .or_else(|| self.concrete_int_bound_type(st, rhs, depth + 1));
                }
                _ => return None,
            },
            _ => {}
        }
        let ty = self.ty_of_expr(st, bare)?;
        self.canonical_go_int_type(&ty)
    }

    fn canonical_go_int_type(&self, ty: &TypeExpr) -> Option<&'static str> {
        let resolved = self.resolve_type_alias(ty);
        let TypeExpr::Named { name, .. } = resolved else {
            return None;
        };
        Some(match name.as_str() {
            "int" | "isize" => "int",
            "uint" | "usize" => "uint",
            "uintptr" => "uintptr",
            "i8" | "int8" => "int8",
            "i16" | "int16" => "int16",
            "i32" | "int32" | "rune" => "int32",
            "i64" | "int64" => "int64",
            "u8" | "uint8" | "byte" => "uint8",
            "u16" | "uint16" => "uint16",
            "u32" | "uint32" => "uint32",
            "u64" | "uint64" => "uint64",
            _ => return None,
        })
    }

    fn const_bound_overflows_range(
        &self,
        st: &FnState,
        bound: &Expr,
        min_v: i128,
        max_v: i128,
    ) -> bool {
        if let Some(v) = self.eval_range_const_int(st, bound, 0) {
            match v {
                RangeConstInt::Val(n) => n < min_v || n > max_v,
                RangeConstInt::Overflow => true,
            }
        } else {
            false
        }
    }

    fn eval_range_const_int(
        &self,
        st: &FnState,
        expr: &Expr,
        depth: u32,
    ) -> Option<RangeConstInt> {
        if depth > 32 {
            return None;
        }
        match unwrap_parens(expr) {
            Expr::Lit {
                kind: LitKind::Int(s),
                ..
            } => Some(match crate::const_overflow::parse_int_lit(s) {
                Some(crate::const_overflow::ConstInt::Value(v)) => RangeConstInt::Val(v),
                _ => RangeConstInt::Overflow,
            }),
            Expr::Lit {
                kind: LitKind::Char(c),
                ..
            } => Some(RangeConstInt::Val(*c as i128)),
            Expr::Ident { name, span } => {
                match self.resolve_scoped_const_in_range(name, span.start) {
                    RangeScopedBinding::ShadowedByRuntimeVar => None,
                    RangeScopedBinding::LocalConst { val, .. } => {
                        self.eval_range_const_int(st, val, depth + 1)
                    }
                    RangeScopedBinding::None => {
                        if st.env_lookup(name).is_some() {
                            return None;
                        }
                        let val = self.const_values.get(name.as_str())?;
                        self.eval_range_const_int(st, val, depth + 1)
                    }
                }
            }
            Expr::Unary {
                op: hy_syntax::UnOp::Neg,
                operand,
                ..
            } => match self.eval_range_const_int(st, operand, depth + 1)? {
                RangeConstInt::Val(v) => Some(match v.checked_neg() {
                    Some(n) => RangeConstInt::Val(n),
                    None => RangeConstInt::Overflow,
                }),
                RangeConstInt::Overflow => Some(RangeConstInt::Overflow),
            },
            Expr::Binary { op, lhs, rhs, .. } => {
                let l = self.eval_range_const_int(st, lhs, depth + 1)?;
                let r = self.eval_range_const_int(st, rhs, depth + 1)?;
                let (RangeConstInt::Val(a), RangeConstInt::Val(b)) = (l, r) else {
                    return Some(RangeConstInt::Overflow);
                };
                match op {
                    hy_syntax::BinOp::Add => Some(
                        a.checked_add(b)
                            .map(RangeConstInt::Val)
                            .unwrap_or(RangeConstInt::Overflow),
                    ),
                    hy_syntax::BinOp::Sub => Some(
                        a.checked_sub(b)
                            .map(RangeConstInt::Val)
                            .unwrap_or(RangeConstInt::Overflow),
                    ),
                    hy_syntax::BinOp::Mul => Some(
                        a.checked_mul(b)
                            .map(RangeConstInt::Val)
                            .unwrap_or(RangeConstInt::Overflow),
                    ),
                    hy_syntax::BinOp::Div => {
                        if b == 0 {
                            None
                        } else {
                            Some(
                                a.checked_div(b)
                                    .map(RangeConstInt::Val)
                                    .unwrap_or(RangeConstInt::Overflow),
                            )
                        }
                    }
                    hy_syntax::BinOp::Mod => {
                        if b == 0 {
                            None
                        } else {
                            a.checked_rem(b).map(RangeConstInt::Val)
                        }
                    }
                    hy_syntax::BinOp::BitAnd => Some(RangeConstInt::Val(a & b)),
                    hy_syntax::BinOp::BitOr => Some(RangeConstInt::Val(a | b)),
                    hy_syntax::BinOp::BitXor => Some(RangeConstInt::Val(a ^ b)),
                    hy_syntax::BinOp::Shl => {
                        if b < 0 {
                            return None;
                        }
                        if a == 0 {
                            return Some(RangeConstInt::Val(0));
                        }
                        let sh = u32::try_from(b).unwrap_or(u32::MAX);
                        if sh < 127 {
                            if let Some(v) = a.checked_shl(sh) {
                                if (a > 0 && v > 0 && (v >> sh) == a)
                                    || (a < 0 && v < 0 && (v >> sh) == a)
                                {
                                    return Some(RangeConstInt::Val(v));
                                }
                            }
                        }
                        Some(RangeConstInt::Overflow)
                    }
                    hy_syntax::BinOp::Shr => {
                        if b < 0 {
                            return None;
                        }
                        let sh = u32::try_from(b).ok()?;
                        if sh >= 127 {
                            Some(RangeConstInt::Val(if a < 0 { -1 } else { 0 }))
                        } else {
                            a.checked_shr(sh).map(RangeConstInt::Val)
                        }
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn resolve_scoped_const_in_range(
        &self,
        name: &str,
        use_pos: hy_diag::Pos,
    ) -> RangeScopedBinding<'a> {
        for decl in &self.file.decls {
            if let hy_syntax::Decl::Fn(f) = decl {
                if range_span_contains(f.span, use_pos) {
                    let mut found = RangeScopedBinding::None;
                    if let Some(r) = &f.receiver {
                        if r.name == name {
                            found = RangeScopedBinding::ShadowedByRuntimeVar;
                        }
                    }
                    for p in &f.params {
                        if p.name == name {
                            found = RangeScopedBinding::ShadowedByRuntimeVar;
                        }
                    }
                    for res in &f.results {
                        if res.name.as_deref() == Some(name) {
                            found = RangeScopedBinding::ShadowedByRuntimeVar;
                        }
                    }
                    if let Some(body) = &f.body {
                        scan_block_for_range_const(body, name, use_pos, &mut found);
                    }
                    return found;
                }
            }
        }
        RangeScopedBinding::None
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

enum RangeScopedBinding<'a> {
    None,
    ShadowedByRuntimeVar,
    LocalConst {
        ty: &'a Option<TypeExpr>,
        val: &'a Expr,
    },
}

enum RangeConstInt {
    Val(i128),
    Overflow,
}

fn range_span_contains(span: Span, pos: hy_diag::Pos) -> bool {
    (span.start.line, span.start.col) <= (pos.line, pos.col)
        && (pos.line, pos.col) <= (span.end.line, span.end.col)
}

fn range_pos_le(a: hy_diag::Pos, b: hy_diag::Pos) -> bool {
    (a.line, a.col) <= (b.line, b.col)
}

fn scan_block_for_range_const<'a>(
    block: &'a hy_syntax::Block,
    name: &str,
    use_pos: hy_diag::Pos,
    found: &mut RangeScopedBinding<'a>,
) {
    if !range_span_contains(block.span, use_pos) {
        return;
    }
    for stmt in &block.stmts {
        if !range_pos_le(stmt.span().start, use_pos) {
            break;
        }
        scan_stmt_for_range_const(stmt, name, use_pos, found);
    }
}

fn scan_stmt_for_range_const<'a>(
    stmt: &'a Stmt,
    name: &str,
    use_pos: hy_diag::Pos,
    found: &mut RangeScopedBinding<'a>,
) {
    match stmt {
        Stmt::Local(l) => {
            if range_pos_le(l.span.end, use_pos) {
                for (idx, n) in l.names.iter().enumerate() {
                    if n == name {
                        if matches!(l.kind, hy_syntax::LocalKind::Const) {
                            if let Some(val) = l.values.get(idx) {
                                *found = RangeScopedBinding::LocalConst { ty: &l.ty, val };
                            }
                        } else {
                            *found = RangeScopedBinding::ShadowedByRuntimeVar;
                        }
                    }
                }
            }
            for v in &l.values {
                if range_span_contains(v.span(), use_pos) {
                    scan_expr_for_range_const(v, name, use_pos, found);
                }
            }
        }
        Stmt::Assign { values, .. } | Stmt::Return { values, .. } => {
            for v in values {
                if range_span_contains(v.span(), use_pos) {
                    scan_expr_for_range_const(v, name, use_pos, found);
                }
            }
        }
        Stmt::Expr { expr, .. } | Stmt::Defer { call: expr, .. } => {
            if range_span_contains(expr.span(), use_pos) {
                scan_expr_for_range_const(expr, name, use_pos, found);
            }
        }
        Stmt::Send { chan, value, .. } => {
            if range_span_contains(chan.span(), use_pos) {
                scan_expr_for_range_const(chan, name, use_pos, found);
            }
            if range_span_contains(value.span(), use_pos) {
                scan_expr_for_range_const(value, name, use_pos, found);
            }
        }
        Stmt::Task { callee, args, .. } => {
            if range_span_contains(callee.span(), use_pos) {
                scan_expr_for_range_const(callee, name, use_pos, found);
            }
            for a in args {
                if range_span_contains(a.value.span(), use_pos) {
                    scan_expr_for_range_const(&a.value, name, use_pos, found);
                }
            }
        }
        Stmt::If {
            init,
            cond,
            then,
            otherwise,
            span,
        } => {
            if range_span_contains(*span, use_pos) {
                if let Some(init_stmt) = init {
                    if range_pos_le(init_stmt.span().start, use_pos) {
                        scan_stmt_for_range_const(init_stmt, name, use_pos, found);
                    }
                }
                if range_span_contains(cond.span(), use_pos) {
                    scan_expr_for_range_const(cond, name, use_pos, found);
                }
                if range_span_contains(then.span, use_pos) {
                    scan_block_for_range_const(then, name, use_pos, found);
                } else if let Some(other) = otherwise {
                    if range_span_contains(other.span(), use_pos) {
                        scan_stmt_for_range_const(other, name, use_pos, found);
                    }
                }
            }
        }
        Stmt::For { kind, body, span } => {
            if range_span_contains(*span, use_pos) {
                match kind {
                    ForKind::While { cond } => {
                        if range_span_contains(cond.span(), use_pos) {
                            scan_expr_for_range_const(cond, name, use_pos, found);
                        }
                    }
                    ForKind::Range { var, lo, hi, .. } => {
                        if range_span_contains(lo.span(), use_pos) {
                            scan_expr_for_range_const(lo, name, use_pos, found);
                        }
                        if range_span_contains(hi.span(), use_pos) {
                            scan_expr_for_range_const(hi, name, use_pos, found);
                        }
                        if range_span_contains(body.span, use_pos) && var == name {
                            *found = RangeScopedBinding::ShadowedByRuntimeVar;
                        }
                    }
                    ForKind::Iterate { vars, expr } => {
                        if range_span_contains(expr.span(), use_pos) {
                            scan_expr_for_range_const(expr, name, use_pos, found);
                        }
                        if range_span_contains(body.span, use_pos)
                            && vars.iter().any(|v| v == name)
                        {
                            *found = RangeScopedBinding::ShadowedByRuntimeVar;
                        }
                    }
                    ForKind::Infinite => {}
                }
                if range_span_contains(body.span, use_pos) {
                    scan_block_for_range_const(body, name, use_pos, found);
                }
            }
        }
        Stmt::Managed { body, .. } | Stmt::Block { block: body, .. } => {
            if range_span_contains(body.span, use_pos) {
                scan_block_for_range_const(body, name, use_pos, found);
            }
        }
        Stmt::Match {
            subject,
            arms,
            span,
        } => {
            if range_span_contains(*span, use_pos) {
                if range_span_contains(subject.span(), use_pos) {
                    scan_expr_for_range_const(subject, name, use_pos, found);
                }
                for arm in arms {
                    let in_guard = arm
                        .guard
                        .as_ref()
                        .is_some_and(|g| range_span_contains(g.span(), use_pos));
                    let in_body = range_span_contains(arm.body.span, use_pos);
                    if in_guard || in_body {
                        if let hy_syntax::MatchPattern::Path {
                            binding: Some(b), ..
                        } = &arm.pattern
                        {
                            if b == name {
                                *found = RangeScopedBinding::ShadowedByRuntimeVar;
                            }
                        }
                        if let Some(g) = &arm.guard {
                            if in_guard {
                                scan_expr_for_range_const(g, name, use_pos, found);
                            }
                        }
                        if in_body {
                            scan_block_for_range_const(&arm.body, name, use_pos, found);
                        }
                    }
                }
            }
        }
        Stmt::Select { arms, span } => {
            if range_span_contains(*span, use_pos) {
                for arm in arms {
                    match &arm.kind {
                        hy_syntax::SelectArmKind::Send { chan, value, .. } => {
                            if range_span_contains(chan.span(), use_pos) {
                                scan_expr_for_range_const(chan, name, use_pos, found);
                            }
                            if range_span_contains(value.span(), use_pos) {
                                scan_expr_for_range_const(value, name, use_pos, found);
                            }
                        }
                        hy_syntax::SelectArmKind::Recv { chan, .. } => {
                            if range_span_contains(chan.span(), use_pos) {
                                scan_expr_for_range_const(chan, name, use_pos, found);
                            }
                        }
                        hy_syntax::SelectArmKind::Default => {}
                    }
                    if range_span_contains(arm.body.span, use_pos) {
                        if let hy_syntax::SelectArmKind::Recv { names, .. } = &arm.kind {
                            if names.iter().any(|n| n == name) {
                                *found = RangeScopedBinding::ShadowedByRuntimeVar;
                            }
                        }
                        scan_block_for_range_const(&arm.body, name, use_pos, found);
                    }
                }
            }
        }
        Stmt::Break { .. } | Stmt::Continue { .. } => {}
    }
}

fn scan_expr_for_range_const<'a>(
    expr: &'a Expr,
    name: &str,
    use_pos: hy_diag::Pos,
    found: &mut RangeScopedBinding<'a>,
) {
    match expr {
        Expr::FnLit {
            params,
            results,
            body,
            span,
        } => {
            if range_span_contains(*span, use_pos) {
                for p in params {
                    if p.name == name {
                        *found = RangeScopedBinding::ShadowedByRuntimeVar;
                    }
                }
                for r in results {
                    if r.name.as_deref() == Some(name) {
                        *found = RangeScopedBinding::ShadowedByRuntimeVar;
                    }
                }
                if range_span_contains(body.span, use_pos) {
                    scan_block_for_range_const(body, name, use_pos, found);
                }
            }
        }
        Expr::Binary { lhs, rhs, .. } | Expr::Index { base: lhs, index: rhs, .. } => {
            if range_span_contains(lhs.span(), use_pos) {
                scan_expr_for_range_const(lhs, name, use_pos, found);
            }
            if range_span_contains(rhs.span(), use_pos) {
                scan_expr_for_range_const(rhs, name, use_pos, found);
            }
        }
        Expr::Unary { operand, .. }
        | Expr::AddrOf { operand, .. }
        | Expr::Move { operand, .. }
        | Expr::Try { operand, .. }
        | Expr::Paren { inner: operand, .. }
        | Expr::Field { base: operand, .. }
        | Expr::Receive { chan: operand, .. } => {
            if range_span_contains(operand.span(), use_pos) {
                scan_expr_for_range_const(operand, name, use_pos, found);
            }
        }
        Expr::Slice { base, lo, hi, .. } => {
            if range_span_contains(base.span(), use_pos) {
                scan_expr_for_range_const(base, name, use_pos, found);
            }
            if let Some(lo) = lo {
                if range_span_contains(lo.span(), use_pos) {
                    scan_expr_for_range_const(lo, name, use_pos, found);
                }
            }
            if let Some(hi) = hi {
                if range_span_contains(hi.span(), use_pos) {
                    scan_expr_for_range_const(hi, name, use_pos, found);
                }
            }
        }
        Expr::Call { callee, args, .. } => {
            if range_span_contains(callee.span(), use_pos) {
                scan_expr_for_range_const(callee, name, use_pos, found);
            }
            for a in args {
                if range_span_contains(a.value.span(), use_pos) {
                    scan_expr_for_range_const(&a.value, name, use_pos, found);
                }
            }
        }
        Expr::StructLit { fields, .. } => {
            for f in fields {
                if range_span_contains(f.value.span(), use_pos) {
                    scan_expr_for_range_const(&f.value, name, use_pos, found);
                }
            }
        }
        Expr::SliceLit { items, .. } => {
            for i in items {
                if range_span_contains(i.span(), use_pos) {
                    scan_expr_for_range_const(i, name, use_pos, found);
                }
            }
        }
        Expr::MapLit { entries, .. } => {
            for (k, v) in entries {
                if range_span_contains(k.span(), use_pos) {
                    scan_expr_for_range_const(k, name, use_pos, found);
                }
                if range_span_contains(v.span(), use_pos) {
                    scan_expr_for_range_const(v, name, use_pos, found);
                }
            }
        }
        Expr::Interp { parts, .. } => {
            for p in parts {
                if let hy_syntax::InterpPart::Expr(e) = p {
                    if range_span_contains(e.span(), use_pos) {
                        scan_expr_for_range_const(e, name, use_pos, found);
                    }
                }
            }
        }
        Expr::Ident { .. } | Expr::Lit { .. } => {}
    }
}

