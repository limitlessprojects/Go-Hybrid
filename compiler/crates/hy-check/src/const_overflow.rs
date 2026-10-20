//! const_overflow.rs — constant initializers that would make the emitted
//! Go unbuildable (issue #148, extended by issue #201-B).
//!
//! `var n int8 = 300` (and the file-level `const` twin) pass `hyb check`
//! with 0 diagnostics today, then fail `go vet` / `go build` with
//! `cannot use 300 as int8 value (overflows)`. The accepted-should-
//! emit-buildable-Go contract requires a checker diagnostic (HYB-E6002)
//! for that narrow constant case. Issue #201-B extends the fold to
//! constant *expressions* (`100 + 100`, `1 << 10` — Go folds them before
//! the range check, so they overflow the same way) and to float constants
//! assigned to an integer type (`2.5` → Go `(truncated)`). Non-constant
//! operands stay out of scope — Go truncates at run time via an explicit
//! conversion (SPEC-LANGUAGE X4), and the conversion form `int8(x)` is
//! unaffected.

use crate::model::{unwrap_parens, Checker, FnState};
use hy_diag::{Diagnostic, Span};
use hy_syntax::{BinOp, CallArg, Expr, ForKind, InterpPart, LitKind, Place, PlaceKind, SelectArmKind, Stmt, TypeExpr, UnOp};
use std::collections::HashMap;

impl<'a> Checker<'a> {
    /// When `ty` is a fixed-width integer and `value` is an integer
    /// constant (literal or folded constant expression) outside that
    /// type's range, or a float constant, emit HYB-E6002.
    pub(crate) fn check_const_overflow_init(
        &mut self,
        name: &str,
        ty: &TypeExpr,
        value: &Expr,
        st: Option<&FnState>,
    ) {
        let resolved = self.resolve_type_alias(ty);
        let TypeExpr::Named {
            name: ty_name,
            span: ty_span,
        } = &resolved
        else {
            return;
        };
        if let Some(is_f32) = is_float_type(ty_name) {
            if let Some(f) = self.const_float_fold(value, st) {
                let overflows = if is_f32 {
                    (f as f32).is_infinite()
                } else {
                    f.is_infinite()
                };
                if overflows {
                    let span = value.span();
                    let msg = format!(
                        "constant `{}` overflows declared type `{}` of `{}` (HYB-E6002)",
                        value.render(),
                        ty_name,
                        name
                    );
                    let notes = vec![
                        format!(
                            "why: Go rejects a constant initializer that overflows `{}` (`cannot use {} as {} value (overflows)`), so an accepted Hybrid program would emit unbuildable Go (issue #683)",
                            ty_name, value.render(), ty_name
                        ),
                        format!(
                            "fix: use a finite `{}` value in range",
                            ty_name
                        ),
                        format!(
                            "note: the declared type was written here ({})",
                            span_hint(*ty_span)
                        ),
                    ];
                    self.diags.push(
                        Diagnostic::error("HYB-E6002", self.path.clone(), span, msg).with_notes(notes),
                    );
                }
            }
            return;
        }
        let Some((lo, hi)) = fixed_width_int_range(ty_name) else {
            return;
        };
        match const_int_fold_with(value, &self.const_values, 0, st) {
            ConstFold::NotConst => {}
            ConstFold::TooBig => {
                let span = value.span();
                let msg = format!(
                    "constant expression `{}` overflows declared type `{}` of `{}` (HYB-E6002)",
                    value.render(),
                    ty_name,
                    name
                );
                let notes = vec![
                    format!(
                        "why: Go folds the constant expression before the range check and rejects a value outside `{}` (`cannot use … as {} value (overflows)`), so an accepted Hybrid program would emit unbuildable Go (issue #201)",
                        ty_name, ty_name
                    ),
                    format!(
                        "fix: use a constant in [{}, {}] or widen the declared type",
                        lo, hi
                    ),
                    format!(
                        "note: the declared type was written here ({})",
                        span_hint(*ty_span)
                    ),
                ];
                self.diags.push(
                    Diagnostic::error("HYB-E6002", self.path.clone(), span, msg).with_notes(notes),
                );
                return;
            }
            ConstFold::Val(v) => {
                if v >= lo && v <= hi {
                    return;
                }
                let span = value.span();
                let msg = format!(
                    "constant `{}` overflows declared type `{}` of `{}` (HYB-E6002)",
                    value.render(),
                    ty_name,
                    name
                );
                let notes = vec![
                    format!(
                        "why: Go rejects a constant initializer outside `{}`'s range (`cannot use … as {} value (overflows)`), so an accepted Hybrid program would emit unbuildable Go (issue #148)",
                        ty_name, ty_name
                    ),
                    format!(
                        "fix: use a constant in [{}, {}], widen the declared type, or convert from a variable if truncation is intentional (`x := {}; y := {}(x)`)",
                        lo, hi, value.render(), ty_name
                    ),
                    format!(
                        "note: the declared type was written here ({})",
                        span_hint(*ty_span)
                    ),
                ];
                self.diags.push(
                    Diagnostic::error("HYB-E6002", self.path.clone(), span, msg).with_notes(notes),
                );
                return;
            }
        }
        // Issue #201-B: a float constant is never assignable to an integer
        // type in Go — `var x int = 2.5` fails with `(truncated)` (and a
        // magnitude that cannot fit fails with `(overflows)`).
        if let Some(f) = const_float_value(value) {
            let span = value.span();
            let msg = format!(
                "float constant `{}` assigned to integer type `{}` of `{}` (HYB-E6002)",
                value.render(),
                ty_name,
                name
            );
            let notes = vec![
                format!(
                    "why: Go rejects a float constant as an `{}` value (`cannot use … as {} value (truncated)`), so an accepted Hybrid program would emit unbuildable Go (issue #201)",
                    ty_name, ty_name
                ),
                format!(
                    "fix: drop the fractional part (`{}` → `{}`), use a float declared type, or convert from a variable if rounding is intentional",
                    value.render(),
                    integer_of_float(f)
                ),
                format!(
                    "note: the declared type was written here ({})",
                    span_hint(*ty_span)
                ),
            ];
            self.diags.push(
                Diagnostic::error("HYB-E6002", self.path.clone(), span, msg).with_notes(notes),
            );
        }
    }

    /// When `ty` is a fixed-width integer and `value` assigned to `target`
    /// is an integer constant outside that type's range, or a float constant, emit HYB-E6002.
    pub(crate) fn check_const_overflow_assign(
        &mut self,
        target: &str,
        ty: &TypeExpr,
        value: &Expr,
    ) {
        let resolved = self.resolve_type_alias(ty);
        let TypeExpr::Named {
            name: ty_name,
            span: ty_span,
        } = &resolved
        else {
            return;
        };
        if let Some(is_f32) = is_float_type(ty_name) {
            if let Some(f) = self.const_float_fold(value, None) {
                let overflows = if is_f32 {
                    (f as f32).is_infinite()
                } else {
                    f.is_infinite()
                };
                if overflows {
                    let span = value.span();
                    let msg = format!(
                        "constant `{}` overflows declared type `{}` of `{}` (HYB-E6002)",
                        value.render(),
                        ty_name,
                        target
                    );
                    let notes = vec![
                        format!(
                            "why: Go rejects a constant assignment that overflows `{}` (`cannot use {} as {} value in assignment (overflows)`), so an accepted Hybrid program would emit unbuildable Go (issue #683)",
                            ty_name, value.render(), ty_name
                        ),
                        format!(
                            "fix: use a finite `{}` value in range",
                            ty_name
                        ),
                        format!(
                            "note: the declared type was written here ({})",
                            span_hint(*ty_span)
                        ),
                    ];
                    self.diags.push(
                        Diagnostic::error("HYB-E6002", self.path.clone(), span, msg).with_notes(notes),
                    );
                }
            }
            return;
        }
        let Some((lo, hi)) = fixed_width_int_range(ty_name) else {
            return;
        };
        match const_int_fold(value) {
            ConstFold::NotConst => {}
            ConstFold::TooBig => {
                let span = value.span();
                let msg = format!(
                    "constant expression `{}` overflows declared type `{}` of `{}` (HYB-E6002)",
                    value.render(),
                    ty_name,
                    target
                );
                let notes = vec![
                    format!(
                        "why: Go folds the constant expression before the range check and rejects a value outside `{}` (`cannot use … as {} value (overflows)`), so an accepted Hybrid program would emit unbuildable Go (issue #300)",
                        ty_name, ty_name
                    ),
                    format!(
                        "fix: use a constant in [{}, {}] or widen the declared type",
                        lo, hi
                    ),
                    format!(
                        "note: the declared type was written here ({})",
                        span_hint(*ty_span)
                    ),
                ];
                self.diags.push(
                    Diagnostic::error("HYB-E6002", self.path.clone(), span, msg).with_notes(notes),
                );
                return;
            }
            ConstFold::Val(v) => {
                if v >= lo && v <= hi {
                    return;
                }
                let span = value.span();
                let msg = format!(
                    "constant `{}` overflows declared type `{}` of `{}` (HYB-E6002)",
                    value.render(),
                    ty_name,
                    target
                );
                let notes = vec![
                    format!(
                        "why: Go rejects a constant assignment outside `{}`'s range (`cannot use … as {} value (overflows)`), so an accepted Hybrid program would emit unbuildable Go (issue #300)",
                        ty_name, ty_name
                    ),
                    format!(
                        "fix: use a constant in [{}, {}], widen the declared type, or convert from a variable if truncation is intentional (`x := {}; {} = {}(x)`)",
                        lo, hi, value.render(), target, ty_name
                    ),
                    format!(
                        "note: the declared type was written here ({})",
                        span_hint(*ty_span)
                    ),
                ];
                self.diags.push(
                    Diagnostic::error("HYB-E6002", self.path.clone(), span, msg).with_notes(notes),
                );
                return;
            }
        }
        if const_float_value(value).is_some() {
            let span = value.span();
            let msg = format!(
                "float constant `{}` assigned to integer type `{}` of `{}` (HYB-E6002)",
                value.render(),
                ty_name,
                target
            );
            let notes = vec![
                format!(
                    "why: Go rejects a float constant as an `{}` value (`cannot use … as {} value (truncated)`), so an accepted Hybrid program would emit unbuildable Go (issue #300)",
                    ty_name, ty_name
                ),
                format!(
                    "fix: convert explicitly at runtime if truncation is intentional (`x := {}; {} = {}(x)`)",
                    value.render(), target, ty_name
                ),
                format!(
                    "note: the declared type was written here ({})",
                    span_hint(*ty_span)
                ),
            ];
            self.diags.push(
                Diagnostic::error("HYB-E6002", self.path.clone(), span, msg).with_notes(notes),
            );
        }
    }

    /// Check integer constants in variable declarations without an explicit
    /// type. Go gives these declarations the target's default `int` type.
    pub(crate) fn check_default_int_overflow(
        &mut self,
        name: &str,
        value: &Expr,
        st: Option<&FnState>,
    ) {
        let (lo, hi) = target_int_range();
        match const_int_fold_with(value, &self.const_values, 0, st) {
            ConstFold::NotConst => return,
            ConstFold::Val(v) if v >= lo && v <= hi => return,
            ConstFold::Val(_) | ConstFold::TooBig => {}
        }

        let span = value.span();
        let msg = format!(
            "constant expression `{}` overflows default type `int` of `{}` (HYB-E6002)",
            value.render(),
            name
        );
        let notes = vec![
            "why: Go assigns default type `int` to an untyped integer constant in a variable declaration and rejects values outside the target int range, so an accepted Hybrid program would emit unbuildable Go (issue #228)"
                .to_string(),
            format!(
                "fix: use a value in [{}, {}], declare an explicit wider type, or keep the value as an untyped constant",
                lo, hi
            ),
        ];
        self.diags.push(
            Diagnostic::error("HYB-E6002", self.path.clone(), span, msg).with_notes(notes),
        );
    }

    /// Check untyped float constants in variable declarations without an explicit
    /// type. Go gives these declarations the default `float64` type.
    pub(crate) fn check_default_float_overflow(
        &mut self,
        name: &str,
        value: &Expr,
        st: Option<&FnState>,
    ) {
        if !is_float_const_expr(value, &self.const_values, 0, st) {
            return;
        }
        if let Some(f) = self.const_float_fold(value, st) {
            if f.is_infinite() {
                let span = value.span();
                let msg = format!(
                    "constant expression `{}` overflows default type `float64` of `{}` (HYB-E6002)",
                    value.render(),
                    name
                );
                let notes = vec![
                    format!(
                        "why: Go assigns default type `float64` to an untyped float constant in a variable declaration and rejects values outside float64's range (`cannot use {} as float64 value in assignment (overflows)`), so an accepted Hybrid program would emit unbuildable Go (issue #683)",
                        value.render()
                    ),
                    "fix: use a finite `float64` value in range, or declare an explicit type".to_string(),
                ];
                self.diags.push(
                    Diagnostic::error("HYB-E6002", self.path.clone(), span, msg).with_notes(notes),
                );
            }
        }
    }

    /// Fold a constant float expression (literals, identifiers, unary, binary).
    pub(crate) fn const_float_fold(&self, e: &Expr, st: Option<&FnState>) -> Option<f64> {
        const_float_fold_with(e, &self.const_values, 0, st)
    }

    // -----------------------------------------------------------------------
    // Issue #228 family C: constant misuse in TYPE CONVERSION CALLS
    // -----------------------------------------------------------------------
    //
    // `int8(300)` (Go spelling), `int8(2.5)`, `int("abc")`, `string(2.5)`,
    // `int(10^30)`, `[]int(3)` all pass `hyb check` with 0 diagnostics and
    // then fail `go build` on the emitted program. SUB-64 integer overflow
    // in a conversion is deliberately NOT rejected here: SPEC-LANGUAGE
    // line 1991's executable claim `u8(300) == 44` makes the emitter fold
    // the two's-complement truncation (constconv.rs), and the same fold now
    // applies to the Go spellings (`int8(300)` -> `int8(44)`). This pass
    // covers the rest of the literal-constant matrix the fold cannot (the
    // same literal-only scope as E6001/E6002 — issue #149/#148).

    /// Pass-B entry: same statement surface (and same deep-closure rule) as
    /// `check_stmt_const_zero_divisor`.
    pub(crate) fn check_stmt_type_conversions(&mut self, stmt: &Stmt) {
        self.check_stmt_type_conversions_inner(stmt, /*deep=*/ false);
    }

    fn check_stmt_type_conversions_inner(&mut self, stmt: &Stmt, deep: bool) {
        match stmt {
            Stmt::Local(l) => {
                for v in &l.values {
                    self.check_expr_type_conversions(v);
                }
            }
            Stmt::Assign { targets, values, .. } => {
                for t in targets {
                    self.check_place_type_conversions(t);
                }
                for v in values {
                    self.check_expr_type_conversions(v);
                }
            }
            Stmt::Expr { expr, .. } => self.check_expr_type_conversions(expr),
            Stmt::Send { chan, value, .. } => {
                self.check_expr_type_conversions(chan);
                self.check_expr_type_conversions(value);
            }
            Stmt::If {
                init,
                cond,
                then,
                otherwise,
                ..
            } => {
                if deep {
                    if let Some(init) = init {
                        self.check_stmt_type_conversions_inner(init, true);
                    }
                }
                self.check_expr_type_conversions(cond);
                if deep {
                    for s in &then.stmts {
                        self.check_stmt_type_conversions_inner(s, true);
                    }
                    if let Some(other) = otherwise {
                        self.check_stmt_type_conversions_inner(other, true);
                    }
                }
            }
            Stmt::For { kind, body, .. } => {
                match kind {
                    ForKind::While { cond } => self.check_expr_type_conversions(cond),
                    ForKind::Range { lo, hi, .. } => {
                        self.check_expr_type_conversions(lo);
                        self.check_expr_type_conversions(hi);
                    }
                    ForKind::Iterate { expr, .. } => self.check_expr_type_conversions(expr),
                    ForKind::Infinite => {}
                }
                if deep {
                    for s in &body.stmts {
                        self.check_stmt_type_conversions_inner(s, true);
                    }
                }
            }
            Stmt::Return { values, .. } => {
                for v in values {
                    self.check_expr_type_conversions(v);
                }
            }
            Stmt::Task { callee, args, .. } => {
                self.check_expr_type_conversions(callee);
                for a in args {
                    self.check_expr_type_conversions(&a.value);
                }
            }
            Stmt::Managed { body, .. } => {
                if deep {
                    for s in &body.stmts {
                        self.check_stmt_type_conversions_inner(s, true);
                    }
                }
            }
            Stmt::Match { subject, arms, .. } => {
                self.check_expr_type_conversions(subject);
                for arm in arms {
                    if let Some(g) = &arm.guard {
                        self.check_expr_type_conversions(g);
                    }
                    if deep {
                        for s in &arm.body.stmts {
                            self.check_stmt_type_conversions_inner(s, true);
                        }
                    }
                }
            }
            Stmt::Defer { call, .. } => self.check_expr_type_conversions(call),
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
            Stmt::Block { block, .. } => {
                if deep {
                    for s in &block.stmts {
                        self.check_stmt_type_conversions_inner(s, true);
                    }
                }
            }
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    match &arm.kind {
                        SelectArmKind::Send { chan, value, is_move: _ } => {
                            self.check_expr_type_conversions(chan);
                            self.check_expr_type_conversions(value);
                        }
                        SelectArmKind::Recv { chan, .. } => self.check_expr_type_conversions(chan),
                        SelectArmKind::Default => {}
                    }
                    if deep {
                        for s in &arm.body.stmts {
                            self.check_stmt_type_conversions_inner(s, true);
                        }
                    }
                }
            }
        }
    }

    fn check_place_type_conversions(&mut self, place: &Place) {
        match &place.kind {
            PlaceKind::Ident(_) => {}
            PlaceKind::Index { base, index } => {
                self.check_place_type_conversions(base);
                self.check_expr_type_conversions(index);
            }
            PlaceKind::Field { base, .. } | PlaceKind::AddrOf { base } => {
                self.check_place_type_conversions(base)
            }
        }
    }

    pub(crate) fn check_expr_type_conversions(&mut self, expr: &Expr) {
        match expr {
            Expr::Binary { lhs, rhs, .. } => {
                self.check_expr_type_conversions(lhs);
                self.check_expr_type_conversions(rhs);
            }
            Expr::Unary { operand, .. }
            | Expr::AddrOf { operand, .. }
            | Expr::Move { operand, .. }
            | Expr::Try { operand, .. }
            | Expr::Paren { inner: operand, .. }
            | Expr::Field { base: operand, .. }
            | Expr::Receive { chan: operand, .. } => {
                self.check_expr_type_conversions(operand)
            }
            Expr::Index { base, index, .. } => {
                self.check_expr_type_conversions(base);
                self.check_expr_type_conversions(index);
            }
            Expr::Slice { base, lo, hi, .. } => {
                self.check_expr_type_conversions(base);
                if let Some(lo) = lo {
                    self.check_expr_type_conversions(lo);
                }
                if let Some(hi) = hi {
                    self.check_expr_type_conversions(hi);
                }
            }
            Expr::Call {
                callee,
                args,
                span,
            } => {
                self.check_expr_type_conversions(callee);
                for a in args {
                    self.check_expr_type_conversions(&a.value);
                }
                self.check_conversion_call(callee, args, *span);
            }
            Expr::StructLit { fields, .. } => {
                for f in fields {
                    self.check_expr_type_conversions(&f.value);
                }
            }
            Expr::SliceLit { items, .. } => {
                for i in items {
                    self.check_expr_type_conversions(i);
                }
            }
            Expr::MapLit { entries, .. } => {
                for (k, v) in entries {
                    self.check_expr_type_conversions(k);
                    self.check_expr_type_conversions(v);
                }
            }
            Expr::Interp { parts, .. } => {
                for p in parts {
                    if let InterpPart::Expr(e) = p {
                        self.check_expr_type_conversions(e);
                    }
                }
            }
            Expr::FnLit { body, .. } => {
                // Closure bodies are not flattened into Pass B — deep-walk.
                for stmt in &body.stmts {
                    self.check_stmt_type_conversions_inner(stmt, /*deep=*/ true);
                }
            }
            Expr::Ident { .. } | Expr::Lit { .. } => {}
        }
    }

    /// One conversion-call site: a one-argument `Type(const)` where Go
    /// would refuse the compile.
    fn check_conversion_call(&mut self, callee: &Expr, args: &[CallArg], span: Span) {
        if args.len() != 1 || args[0].spread {
            // arity of a conversion is the emitter's own guard; spread is
            // not a conversion shape.
            return;
        }
        let arg = unwrap_parens(&args[0].value);
        let bare_callee = unwrap_parens(callee);
        let callee_is_paren = matches!(callee, Expr::Paren { .. });
        // `[]byte("abc")` or `([]int)("abc")` — a slice type-as-value called
        // with one argument (the standing type-as-value encoding; see
        // hy-emit's conversion branch for the emission half of issue #228).
        if let Expr::SliceLit {
            elem: Some(t),
            items,
            ..
        } = bare_callee
        {
            if items.is_empty() && bare_callee.as_array_lit().is_none() {
                self.check_slice_conversion(t, arg, span);
                return;
            }
        }
        let Expr::Ident { name, .. } = bare_callee else {
            return;
        };
        // Resolve type aliases (`type my_int = int`, `type my_slice = []int`).
        let mut resolved_target = name.clone();
        let mut is_alias = false;
        if !is_conversion_target(&resolved_target) {
            if let Some(aliased_ty) = self.type_aliases.get(name).cloned() {
                let resolved_ty = self.resolve_type_alias(&aliased_ty);
                match resolved_ty {
                    TypeExpr::Named {
                        name: inner_name, ..
                    } if is_conversion_target(&inner_name) => {
                        resolved_target = inner_name;
                        is_alias = true;
                    }
                    TypeExpr::Slice { elem, .. } => {
                        self.check_slice_conversion(&elem, arg, span);
                        return;
                    }
                    _ => return,
                }
            } else {
                return;
            }
        }
        let Some(c) = self.conv_const(arg, 0) else {
            return; // non-constant operand: run-time conversion (SPEC X4)
        };
        let skips_sub64_fold =
            callee_is_paren || is_alias || !constconv_folds_expr(arg);
        self.check_conv_against_target(
            name,
            &resolved_target,
            skips_sub64_fold,
            &c,
            arg,
            span,
        );
    }

    fn check_slice_conversion(&mut self, elem: &TypeExpr, arg: &Expr, span: Span) {
        let resolved_elem = self.resolve_type_alias(elem);
        let TypeExpr::Named {
            name: resolved_slice_elem,
            ..
        } = &resolved_elem
        else {
            return; // []T with a non-named element + a run-time operand
        };
        let display_elem = match elem {
            TypeExpr::Named { name, .. } => name.as_str(),
            _ => resolved_slice_elem.as_str(),
        };
        match self.conv_const(arg, 0) {
            Some(ConvConst::Str) => {
                // Go allows string -> []byte / []rune (and their aliases)
                // only; every other element type is a compile error.
                if !matches!(
                    resolved_slice_elem.as_str(),
                    "u8" | "byte" | "uint8" | "i32" | "int32" | "rune"
                ) {
                    self.emit_const_conversion_error(
                        span,
                        format!(
                            "a string constant cannot be converted to a `{}` slice (HYB-E6006)",
                            display_elem
                        ),
                        vec![
                            format!(
                                "why: Go only allows string -> []byte / []rune conversions; `[]{}(\"…\")` fails to build (`cannot convert ... to type []{}`), so an accepted Hybrid program would emit unbuildable Go (issue #228)",
                                display_elem, display_elem
                            ),
                            "fix: convert to `[]byte` (or `[]rune` for text) first, or use a run-time operand whose type matches the target slice".to_string(),
                        ],
                    );
                }
            }
            Some(c) => {
                self.emit_const_conversion_error(
                    span,
                    format!(
                        "a {} constant cannot be converted to a `{}` slice (HYB-E6006)",
                        c.kind_name(),
                        display_elem
                    ),
                    vec![
                        format!(
                            "why: Go has no constant-to-slice conversion; `[]{}({})` fails to build (`invalid operation: cannot convert ...`), so an accepted Hybrid program would emit unbuildable Go (issue #228)",
                            display_elem,
                            arg.render()
                        ),
                        "fix: build the slice explicitly (e.g. `[]{}{{{}}}` or `make([]{}, n)` with a variable length)".to_string(),
                    ],
                );
            }
            None => {} // run-time operand: element-type errors are Go's
        }
    }

    fn check_conv_against_target(
        &mut self,
        display_target: &str,
        resolved_target: &str,
        skips_sub64_fold: bool,
        c: &ConvConst,
        arg: &Expr,
        span: Span,
    ) {
        if let Some((lo, hi)) = fixed_width_int_range(resolved_target) {
            match c {
                ConvConst::Int(v) => {
                    if *v < lo || *v > hi {
                        if is_64bit_int_target(resolved_target) || skips_sub64_fold {
                            // 64-bit targets, parenthesized/aliased callees, and
                            // non-literal constant operands (e.g. `const N = 300; u8(N)`)
                            // skip the emitter's X4 fold — Go rejects them outright.
                            self.emit_const_conversion_error(
                                span,
                                format!(
                                    "constant `{}` overflows `{}` in a conversion (HYB-E6006)",
                                    arg.render(),
                                    display_target
                                ),
                                vec![
                                    format!(
                                        "why: Go rejects `{}({})` at compile time (`constant ... overflows {}`), so an accepted Hybrid program would emit unbuildable Go (issue #228)",
                                        display_target,
                                        arg.render(),
                                        display_target
                                    ),
                                    format!(
                                        "fix: use a constant in [{}, {}], or widen the target type",
                                        lo, hi
                                    ),
                                ],
                            );
                        }
                        // sub-64 literal targets: the emitter's SPEC X4 fold rewrites
                        // the constant (u8(300) == 44) — accepted on purpose.
                    }
                }
                ConvConst::Char(v) => {
                    if (*v as i128) < lo || (*v as i128) > hi {
                        // The fold only rewrites integer LITERALS, so an
                        // out-of-range rune must reject (Go: `constant 256
                        // overflows byte` for `byte('Ā')`).
                        self.emit_const_conversion_error(
                            span,
                            format!(
                                "rune constant `{}` is out of range for `{}` in a conversion (HYB-E6006)",
                                arg.render(),
                                display_target
                            ),
                            vec![
                                format!(
                                    "why: Go rejects `{}({})` at compile time (the code point does not fit `{}`), so an accepted Hybrid program would emit unbuildable Go (issue #228)",
                                    display_target,
                                    arg.render(),
                                    display_target
                                ),
                                format!(
                                    "fix: use a rune in [{}, {}], or convert from a variable if truncation is intended",
                                    lo, hi
                                ),
                            ],
                        );
                    }
                }
                ConvConst::Float(f) => {
                    if !(f.fract() == 0.0 && *f >= lo as f64 && *f <= hi as f64) {
                        self.emit_const_conversion_error(
                            span,
                            format!(
                                "constant `{}` is not representable in `{}` in a conversion (HYB-E6006)",
                                arg.render(),
                                display_target
                            ),
                            vec![
                                format!(
                                    "why: Go only converts an untyped float constant to `{}` when it has no fractional part and fits the range (`cannot convert ...` / `overflows`), so an accepted Hybrid program would emit unbuildable Go (issue #228)",
                                    display_target
                                ),
                                format!(
                                    "fix: use an integer constant in [{}, {}], or bind the value to a variable first",
                                    lo, hi
                                ),
                            ],
                        );
                    }
                }
                ConvConst::Huge { .. } => {
                    // Out of i128 — past every fixed-width target; the
                    // emitter's fold cannot parse it either.
                    self.emit_const_conversion_error(
                        span,
                        format!(
                            "integer constant `{}` does not fit `{}` in a conversion (HYB-E6006)",
                            arg.render(),
                            display_target
                        ),
                        vec![
                            format!(
                                "why: Go rejects `{}({})` at compile time (`constant ... overflows {}`), so an accepted Hybrid program would emit unbuildable Go (issue #228)",
                                display_target,
                                arg.render(),
                                display_target
                            ),
                            format!(
                                "fix: use a constant in [{}, {}], or widen the target type",
                                lo, hi
                            ),
                        ],
                    );
                }
                ConvConst::Str | ConvConst::Bool | ConvConst::Nil => {
                    self.emit_conv_kind_error(display_target, c, arg, span);
                }
            }
            return;
        }
        match resolved_target {
            // float targets: every i128 fits f64; f32's max (3.4e38)
            // exceeds i128's max (1.7e38), so integer/runes always fit.
            // Past-i128 literals carry their f64 reading and still have to
            // respect f32's max (`float32(1e40)` overflows f32 in Go).
            "f32" | "f64" | "float32" | "float64" => match c {
                ConvConst::Float(f) | ConvConst::Huge { f, .. } => {
                    let overflows = !f.is_finite()
                        || (is_f32_target(resolved_target) && f.abs() > f32::MAX as f64);
                    if overflows {
                        self.emit_const_conversion_error(
                            span,
                            format!(
                                "constant `{}` overflows `{}` in a conversion (HYB-E6006)",
                                arg.render(),
                                display_target
                            ),
                            vec![
                                format!(
                                    "why: Go rejects `{}({})` at compile time (`constant ... overflows {}`), so an accepted Hybrid program would emit unbuildable Go (issue #228)",
                                    display_target,
                                    arg.render(),
                                    display_target
                                ),
                                format!(
                                    "fix: use a constant within `{}` range, or widen the target type to `f64`",
                                    display_target
                                ),
                            ],
                        );
                    }
                }
                ConvConst::Str | ConvConst::Bool | ConvConst::Nil => {
                    self.emit_conv_kind_error(display_target, c, arg, span)
                }
                ConvConst::Int(_) | ConvConst::Char(_) => {}
            },
            "string" => match c {
                ConvConst::Str
                | ConvConst::Int(_)
                | ConvConst::Char(_)
                | ConvConst::Huge {
                    lt_512: true, ..
                } => {
                    // Go-legal: `string(65)`, `string("…")`, and huge
                    // rune-valued integers build — verified up to the
                    // 2^512 boundary (go1.25: `string(2^512 - 1)` builds,
                    // `string(2^512)` is `constant overflow`).
                }
                ConvConst::Huge {
                    lt_512: false, ..
                } => {
                    self.emit_const_conversion_error(
                        span,
                        format!(
                            "integer constant `{}` does not fit the conversion to `{}` (HYB-E6006)",
                            arg.render(),
                            display_target
                        ),
                        vec![
                            format!(
                                "why: Go rejects `{}({})` at compile time (`constant overflow` — string conversions accept integer constants below 2^512), so an accepted Hybrid program would emit unbuildable Go (issue #228)",
                                display_target,
                                arg.render()
                            ),
                            "fix: use a smaller constant (below 2^512), or convert from a variable".to_string(),
                        ],
                    );
                }
                ConvConst::Float(_) | ConvConst::Bool | ConvConst::Nil => {
                    self.emit_conv_kind_error(display_target, c, arg, span)
                }
            },
            "bool" => match c {
                ConvConst::Bool => {}
                ConvConst::Int(_)
                | ConvConst::Huge { .. }
                | ConvConst::Float(_)
                | ConvConst::Char(_)
                | ConvConst::Str
                | ConvConst::Nil => self.emit_conv_kind_error(display_target, c, arg, span),
            },
            _ => {}
        }
    }

    fn emit_conv_kind_error(&mut self, target: &str, c: &ConvConst, arg: &Expr, span: Span) {
        self.emit_const_conversion_error(
            span,
            format!(
                "a {} constant cannot be converted to `{}` (HYB-E6006)",
                c.kind_name(),
                target
            ),
            vec![
                format!(
                    "why: Go rejects `{}({})` at compile time (`cannot convert {} to type {}`), so an accepted Hybrid program would emit unbuildable Go (issue #228)",
                    target,
                    arg.render(),
                    arg.render(),
                    target
                ),
                "fix: use a constant of a convertible kind (numbers and strings convert between numeric/string types), or bind the operand to a variable first".to_string(),
            ],
        );
    }

    fn emit_const_conversion_error(&mut self, span: Span, msg: String, notes: Vec<String>) {
        self.diags
            .push(Diagnostic::error("HYB-E6006", self.path.clone(), span, msg).with_notes(notes));
    }

    fn conv_const(&self, e: &Expr, depth: u32) -> Option<ConvConst> {
        if depth > 32 {
            return None;
        }
        match unwrap_parens(e) {
            Expr::Lit { kind, .. } => Some(match kind {
                LitKind::Int(s) => match parse_int_lit(s) {
                    Some(ConstInt::Value(v)) => ConvConst::Int(v),
                    Some(ConstInt::PosOverflow) | Some(ConstInt::NegOverflow) | None => {
                        ConvConst::Huge {
                            f: huge_int_as_f64(s),
                            lt_512: lit_below_2_512(s),
                        }
                    }
                },
                LitKind::Float(s) => ConvConst::Float(s.parse::<f64>().unwrap_or(f64::INFINITY)),
                LitKind::Char(c) => ConvConst::Char(*c as i32),
                LitKind::Str(_) | LitKind::RawStr(_) => ConvConst::Str,
                LitKind::Bool(_) => ConvConst::Bool,
                LitKind::Nil => ConvConst::Nil,
            }),
            Expr::Ident { name, span } => {
                if name == "true" || name == "false" {
                    return Some(ConvConst::Bool);
                }
                if name == "nil" {
                    return Some(ConvConst::Nil);
                }
                match self.resolve_scoped_const_for_conv(name, span.start) {
                    ScopedConvBinding::ShadowedByRuntimeVar => return None,
                    ScopedConvBinding::LocalConst { ty, val } => {
                        let c = self.conv_const(val, depth + 1)?;
                        return Some(self.coerce_conv_const_for_decl_ty(c, ty.as_ref()));
                    }
                    ScopedConvBinding::None => {}
                }
                if let Some(val) = self.const_values.get(name.as_str()) {
                    let c = self.conv_const(val, depth + 1)?;
                    let decl_ty = self.consts.get(name.as_str()).and_then(|b| b.ty.as_ref());
                    return Some(self.coerce_conv_const_for_decl_ty(c, decl_ty));
                }
                None
            }
            Expr::Unary { op, operand, .. } => {
                let inner = self.conv_const(unwrap_parens(operand), depth + 1)?;
                match op {
                    UnOp::Neg => match inner {
                        ConvConst::Int(v) => Some(match v.checked_neg() {
                            Some(n) => ConvConst::Int(n),
                            None => ConvConst::Huge {
                                f: -(v as f64),
                                lt_512: true,
                            },
                        }),
                        ConvConst::Char(c) => Some(ConvConst::Int(-(c as i128))),
                        ConvConst::Float(f) => Some(ConvConst::Float(-f)),
                        ConvConst::Huge { f, lt_512 } => Some(ConvConst::Huge { f: -f, lt_512 }),
                        other => Some(other),
                    },
                    UnOp::Not => match inner {
                        ConvConst::Bool => Some(ConvConst::Bool),
                        _ => None,
                    },
                    UnOp::Addr => None,
                }
            }
            Expr::Binary { op, lhs, rhs, .. } => {
                let l = self.conv_const(lhs, depth + 1)?;
                let r = self.conv_const(rhs, depth + 1)?;
                match op {
                    BinOp::Eq
                    | BinOp::Ne
                    | BinOp::Lt
                    | BinOp::Le
                    | BinOp::Gt
                    | BinOp::Ge
                    | BinOp::And
                    | BinOp::Or => Some(ConvConst::Bool),
                    BinOp::Add if matches!((&l, &r), (ConvConst::Str, ConvConst::Str)) => {
                        Some(ConvConst::Str)
                    }
                    BinOp::Add
                    | BinOp::Sub
                    | BinOp::Mul
                    | BinOp::Div
                    | BinOp::Mod
                    | BinOp::BitAnd
                    | BinOp::BitOr
                    | BinOp::BitXor
                    | BinOp::Shl
                    | BinOp::Shr => eval_conv_binary(*op, l, r),
                }
            }
            _ => None,
        }
    }

    fn coerce_conv_const_for_decl_ty(
        &self,
        c: ConvConst,
        decl_ty: Option<&TypeExpr>,
    ) -> ConvConst {
        let Some(ty) = decl_ty else {
            return c;
        };
        let resolved = self.resolve_type_alias(ty);
        if let TypeExpr::Named { name, .. } = resolved {
            if matches!(name.as_str(), "f32" | "f64" | "float32" | "float64") {
                match c {
                    ConvConst::Int(v) => return ConvConst::Float(v as f64),
                    ConvConst::Char(v) => return ConvConst::Float(v as f64),
                    ConvConst::Huge { f, .. } => return ConvConst::Float(f),
                    other => return other,
                }
            }
        }
        c
    }

    fn resolve_scoped_const_for_conv(
        &self,
        name: &str,
        use_pos: hy_diag::Pos,
    ) -> ScopedConvBinding<'a> {
        for decl in &self.file.decls {
            if let hy_syntax::Decl::Fn(f) = decl {
                if conv_span_contains(f.span, use_pos) {
                    let mut found = ScopedConvBinding::None;
                    if let Some(r) = &f.receiver {
                        if r.name == name {
                            found = ScopedConvBinding::ShadowedByRuntimeVar;
                        }
                    }
                    for p in &f.params {
                        if p.name == name {
                            found = ScopedConvBinding::ShadowedByRuntimeVar;
                        }
                    }
                    for res in &f.results {
                        if res.name.as_deref() == Some(name) {
                            found = ScopedConvBinding::ShadowedByRuntimeVar;
                        }
                    }
                    if let Some(body) = &f.body {
                        scan_block_for_conv_binding(body, name, use_pos, &mut found);
                    }
                    return found;
                }
            }
        }
        ScopedConvBinding::None
    }
}

enum ScopedConvBinding<'a> {
    None,
    ShadowedByRuntimeVar,
    LocalConst {
        ty: &'a Option<TypeExpr>,
        val: &'a Expr,
    },
}

fn conv_span_contains(span: Span, pos: hy_diag::Pos) -> bool {
    (span.start.line, span.start.col) <= (pos.line, pos.col)
        && (pos.line, pos.col) <= (span.end.line, span.end.col)
}

fn conv_pos_le(a: hy_diag::Pos, b: hy_diag::Pos) -> bool {
    (a.line, a.col) <= (b.line, b.col)
}

fn scan_block_for_conv_binding<'a>(
    block: &'a hy_syntax::Block,
    name: &str,
    use_pos: hy_diag::Pos,
    found: &mut ScopedConvBinding<'a>,
) {
    if !conv_span_contains(block.span, use_pos) {
        return;
    }
    for stmt in &block.stmts {
        if !conv_pos_le(stmt.span().start, use_pos) {
            break;
        }
        scan_stmt_for_conv_binding(stmt, name, use_pos, found);
    }
}

fn scan_stmt_for_conv_binding<'a>(
    stmt: &'a Stmt,
    name: &str,
    use_pos: hy_diag::Pos,
    found: &mut ScopedConvBinding<'a>,
) {
    match stmt {
        Stmt::Local(l) => {
            if conv_pos_le(l.span.end, use_pos) {
                for (idx, n) in l.names.iter().enumerate() {
                    if n == name {
                        if matches!(l.kind, hy_syntax::LocalKind::Const) {
                            if let Some(val) = l.values.get(idx) {
                                *found = ScopedConvBinding::LocalConst { ty: &l.ty, val };
                            }
                        } else {
                            *found = ScopedConvBinding::ShadowedByRuntimeVar;
                        }
                    }
                }
            }
            for v in &l.values {
                if conv_span_contains(v.span(), use_pos) {
                    scan_expr_for_conv_binding(v, name, use_pos, found);
                }
            }
        }
        Stmt::Assign { values, .. } | Stmt::Return { values, .. } => {
            for v in values {
                if conv_span_contains(v.span(), use_pos) {
                    scan_expr_for_conv_binding(v, name, use_pos, found);
                }
            }
        }
        Stmt::Expr { expr, .. } | Stmt::Defer { call: expr, .. } => {
            if conv_span_contains(expr.span(), use_pos) {
                scan_expr_for_conv_binding(expr, name, use_pos, found);
            }
        }
        Stmt::Send { chan, value, .. } => {
            if conv_span_contains(chan.span(), use_pos) {
                scan_expr_for_conv_binding(chan, name, use_pos, found);
            }
            if conv_span_contains(value.span(), use_pos) {
                scan_expr_for_conv_binding(value, name, use_pos, found);
            }
        }
        Stmt::Task { callee, args, .. } => {
            if conv_span_contains(callee.span(), use_pos) {
                scan_expr_for_conv_binding(callee, name, use_pos, found);
            }
            for a in args {
                if conv_span_contains(a.value.span(), use_pos) {
                    scan_expr_for_conv_binding(&a.value, name, use_pos, found);
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
            if conv_span_contains(*span, use_pos) {
                if let Some(init_stmt) = init {
                    if conv_pos_le(init_stmt.span().start, use_pos) {
                        scan_stmt_for_conv_binding(init_stmt, name, use_pos, found);
                    }
                }
                if conv_span_contains(cond.span(), use_pos) {
                    scan_expr_for_conv_binding(cond, name, use_pos, found);
                }
                if conv_span_contains(then.span, use_pos) {
                    scan_block_for_conv_binding(then, name, use_pos, found);
                } else if let Some(other) = otherwise {
                    if conv_span_contains(other.span(), use_pos) {
                        scan_stmt_for_conv_binding(other, name, use_pos, found);
                    }
                }
            }
        }
        Stmt::For { kind, body, span } => {
            if conv_span_contains(*span, use_pos) {
                match kind {
                    ForKind::While { cond } => {
                        if conv_span_contains(cond.span(), use_pos) {
                            scan_expr_for_conv_binding(cond, name, use_pos, found);
                        }
                    }
                    ForKind::Range { var, lo, hi, .. } => {
                        if conv_span_contains(lo.span(), use_pos) {
                            scan_expr_for_conv_binding(lo, name, use_pos, found);
                        }
                        if conv_span_contains(hi.span(), use_pos) {
                            scan_expr_for_conv_binding(hi, name, use_pos, found);
                        }
                        if conv_span_contains(body.span, use_pos) && var == name {
                            *found = ScopedConvBinding::ShadowedByRuntimeVar;
                        }
                    }
                    ForKind::Iterate { vars, expr } => {
                        if conv_span_contains(expr.span(), use_pos) {
                            scan_expr_for_conv_binding(expr, name, use_pos, found);
                        }
                        if conv_span_contains(body.span, use_pos)
                            && vars.iter().any(|v| v == name)
                        {
                            *found = ScopedConvBinding::ShadowedByRuntimeVar;
                        }
                    }
                    ForKind::Infinite => {}
                }
                if conv_span_contains(body.span, use_pos) {
                    scan_block_for_conv_binding(body, name, use_pos, found);
                }
            }
        }
        Stmt::Managed { body, .. } | Stmt::Block { block: body, .. } => {
            if conv_span_contains(body.span, use_pos) {
                scan_block_for_conv_binding(body, name, use_pos, found);
            }
        }
        Stmt::Match {
            subject,
            arms,
            span,
        } => {
            if conv_span_contains(*span, use_pos) {
                if conv_span_contains(subject.span(), use_pos) {
                    scan_expr_for_conv_binding(subject, name, use_pos, found);
                }
                for arm in arms {
                    let in_guard = arm
                        .guard
                        .as_ref()
                        .is_some_and(|g| conv_span_contains(g.span(), use_pos));
                    let in_body = conv_span_contains(arm.body.span, use_pos);
                    if in_guard || in_body {
                        if let hy_syntax::MatchPattern::Path {
                            binding: Some(b), ..
                        } = &arm.pattern
                        {
                            if b == name {
                                *found = ScopedConvBinding::ShadowedByRuntimeVar;
                            }
                        }
                        if let Some(g) = &arm.guard {
                            if in_guard {
                                scan_expr_for_conv_binding(g, name, use_pos, found);
                            }
                        }
                        if in_body {
                            scan_block_for_conv_binding(&arm.body, name, use_pos, found);
                        }
                    }
                }
            }
        }
        Stmt::Select { arms, span } => {
            if conv_span_contains(*span, use_pos) {
                for arm in arms {
                    match &arm.kind {
                        SelectArmKind::Send { chan, value, .. } => {
                            if conv_span_contains(chan.span(), use_pos) {
                                scan_expr_for_conv_binding(chan, name, use_pos, found);
                            }
                            if conv_span_contains(value.span(), use_pos) {
                                scan_expr_for_conv_binding(value, name, use_pos, found);
                            }
                        }
                        SelectArmKind::Recv { chan, .. } => {
                            if conv_span_contains(chan.span(), use_pos) {
                                scan_expr_for_conv_binding(chan, name, use_pos, found);
                            }
                        }
                        SelectArmKind::Default => {}
                    }
                    if conv_span_contains(arm.body.span, use_pos) {
                        if let SelectArmKind::Recv { names, .. } = &arm.kind {
                            if names.iter().any(|n| n == name) {
                                *found = ScopedConvBinding::ShadowedByRuntimeVar;
                            }
                        }
                        scan_block_for_conv_binding(&arm.body, name, use_pos, found);
                    }
                }
            }
        }
        Stmt::Break { .. } | Stmt::Continue { .. } => {}
    }
}

fn scan_expr_for_conv_binding<'a>(
    expr: &'a Expr,
    name: &str,
    use_pos: hy_diag::Pos,
    found: &mut ScopedConvBinding<'a>,
) {
    match expr {
        Expr::FnLit {
            params,
            results,
            body,
            span,
        } => {
            if conv_span_contains(*span, use_pos) {
                for p in params {
                    if p.name == name {
                        *found = ScopedConvBinding::ShadowedByRuntimeVar;
                    }
                }
                for r in results {
                    if r.name.as_deref() == Some(name) {
                        *found = ScopedConvBinding::ShadowedByRuntimeVar;
                    }
                }
                if conv_span_contains(body.span, use_pos) {
                    scan_block_for_conv_binding(body, name, use_pos, found);
                }
            }
        }
        Expr::Binary { lhs, rhs, .. } | Expr::Index { base: lhs, index: rhs, .. } => {
            if conv_span_contains(lhs.span(), use_pos) {
                scan_expr_for_conv_binding(lhs, name, use_pos, found);
            }
            if conv_span_contains(rhs.span(), use_pos) {
                scan_expr_for_conv_binding(rhs, name, use_pos, found);
            }
        }
        Expr::Unary { operand, .. }
        | Expr::AddrOf { operand, .. }
        | Expr::Move { operand, .. }
        | Expr::Try { operand, .. }
        | Expr::Paren { inner: operand, .. }
        | Expr::Field { base: operand, .. }
        | Expr::Receive { chan: operand, .. } => {
            if conv_span_contains(operand.span(), use_pos) {
                scan_expr_for_conv_binding(operand, name, use_pos, found);
            }
        }
        Expr::Slice { base, lo, hi, .. } => {
            if conv_span_contains(base.span(), use_pos) {
                scan_expr_for_conv_binding(base, name, use_pos, found);
            }
            if let Some(lo) = lo {
                if conv_span_contains(lo.span(), use_pos) {
                    scan_expr_for_conv_binding(lo, name, use_pos, found);
                }
            }
            if let Some(hi) = hi {
                if conv_span_contains(hi.span(), use_pos) {
                    scan_expr_for_conv_binding(hi, name, use_pos, found);
                }
            }
        }
        Expr::Call { callee, args, .. } => {
            if conv_span_contains(callee.span(), use_pos) {
                scan_expr_for_conv_binding(callee, name, use_pos, found);
            }
            for a in args {
                if conv_span_contains(a.value.span(), use_pos) {
                    scan_expr_for_conv_binding(&a.value, name, use_pos, found);
                }
            }
        }
        Expr::StructLit { fields, .. } => {
            for f in fields {
                if conv_span_contains(f.value.span(), use_pos) {
                    scan_expr_for_conv_binding(&f.value, name, use_pos, found);
                }
            }
        }
        Expr::SliceLit { items, .. } => {
            for i in items {
                if conv_span_contains(i.span(), use_pos) {
                    scan_expr_for_conv_binding(i, name, use_pos, found);
                }
            }
        }
        Expr::MapLit { entries, .. } => {
            for (k, v) in entries {
                if conv_span_contains(k.span(), use_pos) {
                    scan_expr_for_conv_binding(k, name, use_pos, found);
                }
                if conv_span_contains(v.span(), use_pos) {
                    scan_expr_for_conv_binding(v, name, use_pos, found);
                }
            }
        }
        Expr::Interp { parts, .. } => {
            for p in parts {
                if let InterpPart::Expr(e) = p {
                    if conv_span_contains(e.span(), use_pos) {
                        scan_expr_for_conv_binding(e, name, use_pos, found);
                    }
                }
            }
        }
        Expr::Ident { .. } | Expr::Lit { .. } => {}
    }
}

/// True when `hy-emit::constconv::fold_constant` can fold `expr` as a literal-only
/// integer expression (no `const` identifiers, no floats).
fn constconv_folds_expr(expr: &Expr) -> bool {
    match expr {
        Expr::Paren { inner, .. } => constconv_folds_expr(inner),
        Expr::Unary {
            op: UnOp::Neg,
            operand,
            ..
        } => constconv_folds_expr(operand),
        Expr::Lit {
            kind: LitKind::Int(s),
            ..
        } => matches!(parse_int_lit(s), Some(ConstInt::Value(_))),
        Expr::Lit {
            kind: LitKind::Char(_),
            ..
        } => true,
        Expr::Binary { op, lhs, rhs, .. } => {
            matches!(
                op,
                BinOp::Add
                    | BinOp::Sub
                    | BinOp::Mul
                    | BinOp::Div
                    | BinOp::Mod
                    | BinOp::BitAnd
                    | BinOp::BitOr
                    | BinOp::BitXor
                    | BinOp::Shl
                    | BinOp::Shr
            ) && constconv_folds_expr(lhs)
                && constconv_folds_expr(rhs)
        }
        _ => false,
    }
}

fn eval_conv_binary(op: BinOp, l: ConvConst, r: ConvConst) -> Option<ConvConst> {
    let as_int = |c: &ConvConst| -> Option<i128> {
        match c {
            ConvConst::Int(v) => Some(*v),
            ConvConst::Char(v) => Some(*v as i128),
            _ => None,
        }
    };
    if let (Some(a), Some(b)) = (as_int(&l), as_int(&r)) {
        return match op {
            BinOp::Add => Some(match a.checked_add(b) {
                Some(v) => ConvConst::Int(v),
                None => ConvConst::Huge {
                    f: (a as f64) + (b as f64),
                    lt_512: true,
                },
            }),
            BinOp::Sub => Some(match a.checked_sub(b) {
                Some(v) => ConvConst::Int(v),
                None => ConvConst::Huge {
                    f: (a as f64) - (b as f64),
                    lt_512: true,
                },
            }),
            BinOp::Mul => Some(match a.checked_mul(b) {
                Some(v) => ConvConst::Int(v),
                None => {
                    let f = (a as f64) * (b as f64);
                    ConvConst::Huge {
                        f,
                        lt_512: f.is_finite() && f.abs() < 2f64.powi(512),
                    }
                }
            }),
            BinOp::Div => {
                if b == 0 {
                    None
                } else {
                    Some(match a.checked_div(b) {
                        Some(v) => ConvConst::Int(v),
                        None => ConvConst::Huge {
                            f: -(a as f64),
                            lt_512: true,
                        },
                    })
                }
            }
            BinOp::Mod => {
                if b == 0 {
                    None
                } else {
                    a.checked_rem(b).map(ConvConst::Int)
                }
            }
            BinOp::BitAnd => Some(ConvConst::Int(a & b)),
            BinOp::BitOr => Some(ConvConst::Int(a | b)),
            BinOp::BitXor => Some(ConvConst::Int(a ^ b)),
            BinOp::Shl => {
                if b < 0 {
                    return None;
                }
                let sh = u32::try_from(b).unwrap_or(u32::MAX);
                if a == 0 {
                    return Some(ConvConst::Int(0));
                }
                if sh < 127 {
                    if let Some(v) = a.checked_shl(sh) {
                        // Ensure sign did not flip (`1i128 << 127` is negative).
                        if (a > 0 && v > 0 && (v >> sh) == a)
                            || (a < 0 && v < 0 && (v >> sh) == a)
                        {
                            return Some(ConvConst::Int(v));
                        }
                    }
                }
                let shift_f = if sh > 1024 {
                    f64::INFINITY
                } else {
                    2f64.powi(sh as i32)
                };
                let f = (a as f64) * shift_f;
                let abs_a = a.unsigned_abs();
                let bit_len = 128 - abs_a.leading_zeros();
                let lt_512 = sh.saturating_add(bit_len) <= 512;
                Some(ConvConst::Huge { f, lt_512 })
            }
            BinOp::Shr => {
                if b < 0 {
                    return None;
                }
                let sh = u32::try_from(b).ok()?;
                if sh >= 127 {
                    Some(ConvConst::Int(if a < 0 { -1 } else { 0 }))
                } else {
                    a.checked_shr(sh).map(ConvConst::Int)
                }
            }
            _ => None,
        };
    }
    let as_float = |c: &ConvConst| -> Option<f64> {
        match c {
            ConvConst::Float(f) => Some(*f),
            ConvConst::Int(v) => Some(*v as f64),
            ConvConst::Char(v) => Some(*v as f64),
            ConvConst::Huge { f, .. } => Some(*f),
            _ => None,
        }
    };
    if matches!(l, ConvConst::Float(_))
        || matches!(r, ConvConst::Float(_))
        || matches!((&l, &r), (ConvConst::Huge { .. }, _) | (_, ConvConst::Huge { .. }))
    {
        let lf = as_float(&l)?;
        let rf = as_float(&r)?;
        return match op {
            BinOp::Add => Some(ConvConst::Float(lf + rf)),
            BinOp::Sub => Some(ConvConst::Float(lf - rf)),
            BinOp::Mul => Some(ConvConst::Float(lf * rf)),
            BinOp::Div => {
                if rf == 0.0 {
                    None
                } else {
                    Some(ConvConst::Float(lf / rf))
                }
            }
            _ => None,
        };
    }
    None
}

enum ConvConst {
    Int(i128),
    /// Integer literal/expression past i128 — carries its `f64` reading
    /// so float targets can still decide (`f64(10^40)` is fine, `f32(10^40)`
    /// overflows f32), plus whether the value is strictly below 2^512
    /// (Go's verified boundary: `string(2^512 - 1)` builds,
    /// `string(2^512)` is `constant overflow`).
    Huge { f: f64, lt_512: bool },
    Float(f64),
    Char(i32),
    Str,
    Bool,
    Nil,
}

impl ConvConst {
    fn kind_name(&self) -> &'static str {
        match self {
            ConvConst::Int(_) | ConvConst::Huge { .. } => "integer",
            ConvConst::Float(_) => "float",
            ConvConst::Char(_) => "rune",
            ConvConst::Str => "string",
            ConvConst::Bool => "boolean",
            ConvConst::Nil => "nil",
        }
    }
}

/// Strictly-below-2^512, decided EXACTLY on the literal's digit string (an
/// `f64` reading would round `2^512 - 1` up to `2^512` and wrongly reject
/// what Go builds). Verified against go1.25: `string(2^512 - 1)` builds,
/// `string(2^512)` is `constant overflow`; the boundary does not appear on
/// any other target (i128 tops out at ~1.7e38, f64 at ~1.8e308).
const TWO_POW_512: &str = "13407807929942597099574024998205846127479365820592393377723561443721764030073546976801874298166903427690031858186486050853753882811946569946433649006084096";

fn lit_below_2_512(s: &str) -> bool {
    let t = s.replace('_', "");
    let t = t.trim_start_matches('0');
    let t = if t.is_empty() { "0" } else { t };
    let t = t.strip_prefix('-').unwrap_or(t);
    match t.len().cmp(&TWO_POW_512.len()) {
        std::cmp::Ordering::Less => true,
        std::cmp::Ordering::Greater => false,
        std::cmp::Ordering::Equal => t < TWO_POW_512,
    }
}

/// Best `f64` reading of an integer literal that overflowed `i128` — used so
/// float targets can still decide (`f64(10^40)` is fine, `f32(10^40)`
/// overflows f32). Radix forms are read as `u128` first (they top out at
/// 3.4e38, inside f64); decimals read directly (anything past 1e308
/// legitimately becomes `inf`, which is exactly the Go-side rejection).
fn huge_int_as_f64(s: &str) -> f64 {
    let t = s.replace('_', "");
    if let Ok(v) = t.parse::<f64>() {
        return v;
    }
    for (pref, radix) in [("0x", 16u32), ("0X", 16), ("0b", 2), ("0B", 2), ("0o", 8), ("0O", 8)] {
        if let Some(rest) = t.strip_prefix(pref) {
            if let Ok(v) = u128::from_str_radix(rest, radix) {
                return v as f64;
            }
        }
    }
    f64::INFINITY
}

/// True when `name` is a built-in conversion target of `Type(const)` calls.
fn is_conversion_target(name: &str) -> bool {
    fixed_width_int_range(name).is_some()
        || matches!(name, "f32" | "f64" | "float32" | "float64" | "string" | "bool")
}

/// Targets the emitter's SPEC X4 fold skips (`bits >= 64`) — constants here
/// must reject instead of pass through.
fn is_64bit_int_target(name: &str) -> bool {
    matches!(
        name,
        "i64" | "int64" | "int" | "isize" | "u64" | "uint64" | "uint" | "usize" | "uintptr"
    )
}

fn is_f32_target(name: &str) -> bool {
    matches!(name, "f32" | "float32")
}

/// The built-in float TYPES as spelled in an ascription: `Some(true)` =
/// float32, `Some(false)` = float64. `f32`/`f64` are accepted alias
/// spellings (the emitter maps both to float32/float64 — issue #683's
/// float legs must reject past-range constants under either spelling).
/// Carried from rickymulya's parallel take #689 (credit).
fn is_float_type(ty_name: &str) -> Option<bool> {
    match ty_name {
        "float32" | "f32" => Some(true),
        "float64" | "f64" => Some(false),
        _ => None,
    }
}

fn span_hint(span: Span) -> String {
    format!("{}:{}", span.start.line, span.start.col)
}

/// Target word size for platform-dependent integer types. Hybrid's current
/// emitter/runtime targets are 64-bit; keep the policy centralized so a
/// future target configuration can replace this constant.
pub(crate) const TARGET_INT_WIDTH_BITS: u32 = 64;

#[inline]
pub(crate) fn target_int_range() -> (i128, i128) {
    match TARGET_INT_WIDTH_BITS {
        32 => (i32::MIN as i128, i32::MAX as i128),
        64 => (i64::MIN as i128, i64::MAX as i128),
        _ => (i64::MIN as i128, i64::MAX as i128),
    }
}

#[inline]
pub(crate) fn target_uint_range() -> (i128, i128) {
    match TARGET_INT_WIDTH_BITS {
        32 => (0, u32::MAX as i128),
        64 => (0, u64::MAX as i128),
        _ => (0, u64::MAX as i128),
    }
}

/// Inclusive representable range for a Hybrid fixed-width integer type
/// name (SPEC-LANGUAGE §11 / Go's typed-constant assignment rule).
/// Platform-word `int`/`uint`/`isize`/`usize` use the 64-bit LP64 ranges
/// the emitter targets (SPEC B9 / X7).
pub(crate) fn fixed_width_int_range(name: &str) -> Option<(i128, i128)> {
    Some(match name {
        "i8" | "int8" => (i8::MIN as i128, i8::MAX as i128),
        "u8" | "uint8" | "byte" => (0, u8::MAX as i128),
        "i16" | "int16" => (i16::MIN as i128, i16::MAX as i128),
        "u16" | "uint16" => (0, u16::MAX as i128),
        "i32" | "int32" | "rune" => (i32::MIN as i128, i32::MAX as i128),
        "u32" | "uint32" => (0, u32::MAX as i128),
        "i64" | "int64" => (i64::MIN as i128, i64::MAX as i128),
        "int" | "isize" => target_int_range(),
        "u64" | "uint64" => (0, u64::MAX as i128),
        "uint" | "usize" | "uintptr" => target_uint_range(),
        _ => return None,
    })
}

/// Parsed integer literal, retaining the fact that a syntactically valid
/// value exceeded i128 instead of treating it as non-constant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConstInt {
    Value(i128),
    PosOverflow,
    NegOverflow,
}

/// Outcome of folding a constant integer expression (issue #201-B).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConstFold {
    /// Folded to an exact i128 value.
    Val(i128),
    /// Not a constant the checker can prove (idents, calls, …) — callers
    /// stay silent (fail-open; other diagnostics own those shapes).
    NotConst,
    /// Provably a constant whose magnitude exceeds every fixed-width
    /// Hybrid integer range (e.g. `1 << 200`, `i128` arithmetic overflow).
    TooBig,
}

/// Fold a pure integer constant expression: int literals (any base the
/// lexer keeps), unary `-`, parentheses, and the arithmetic/shift/bitwise
/// operators over constant operands (issue #201-B: Go folds `100 + 100`
/// and `1 << 10` before the range check, so they overflow identically).
/// Anything else (idents, calls) is `NotConst`.
pub(crate) fn const_int_fold(e: &Expr) -> ConstFold {
    const_int_fold_with(e, &HashMap::new(), 0, None)
}

/// Like [`const_int_fold`], but resolves file-level `const` identifiers to
/// their (constant) initializers: `const big = 300; var n int8 = big` must
/// be rejected like the literal. A local binding with the same name shadows
/// the file-level const, so `st` is consulted before resolving.
pub(crate) fn const_int_fold_with(
    e: &Expr,
    consts: &HashMap<String, &Expr>,
    depth: u32,
    st: Option<&FnState>,
) -> ConstFold {
    match unwrap_parens(e) {
        Expr::Lit {
            kind: LitKind::Int(s),
            ..
        } => match parse_int_lit(s) {
            Some(ConstInt::Value(v)) => ConstFold::Val(v),
            Some(ConstInt::PosOverflow | ConstInt::NegOverflow) => ConstFold::TooBig,
            None => ConstFold::NotConst,
        },
        Expr::Lit {
            kind: LitKind::Char(c),
            ..
        } => ConstFold::Val(*c as i128),
        Expr::Ident { name, .. } => {
            if let Some(st) = st {
                if st.env_lookup(name).is_some() {
                    return ConstFold::NotConst;
                }
            }
            if depth < 16 {
                if let Some(v) = consts.get(name) {
                    // The function state guards the identifier at the *use
                    // site* only: a const's own initializer refers to the
                    // file-level scope, so expand it without `st`.
                    return const_int_fold_with(v, consts, depth + 1, None);
                }
            }
            ConstFold::NotConst
        }
        Expr::Unary {
            op: UnOp::Neg,
            operand,
            ..
        } => match const_int_fold_with(operand, consts, depth + 1, st) {
            ConstFold::Val(v) => v.checked_neg().map(ConstFold::Val).unwrap_or(ConstFold::TooBig),
            ConstFold::NotConst => ConstFold::NotConst,
            ConstFold::TooBig => ConstFold::TooBig,
        },
        Expr::Binary { op, lhs, rhs, .. } => {
            let l = const_int_fold_with(lhs, consts, depth + 1, st);
            let r = const_int_fold_with(rhs, consts, depth + 1, st);
            fold_bin(*op, l, r)
        }
        // A fixed-width integer conversion of a constant is ITSELF a
        // constant, and SPEC X4 gives it the truncated value: `u8(300)` is
        // 44, not 300 (`hy-emit`'s `constconv` fold already prints
        // `uint8(44)` for it, so this only teaches the checker what the
        // emitter was already doing).
        //
        // Without this arm a typed constant expression does not fold at all
        // — `u8(255) + 1` sees a `Call` on the left, gives up, and the
        // result-range check downstream has no value to range-check
        // (issue #725).
        Expr::Call { callee, args, .. } => {
            if args.len() != 1 {
                return ConstFold::NotConst;
            }
            let Expr::Ident { name, .. } = unwrap_parens(callee) else {
                return ConstFold::NotConst;
            };
            let Some((lo, hi)) = fixed_width_int_range(name) else {
                return ConstFold::NotConst;
            };
            match const_int_fold_with(&args[0].value, consts, depth + 1, st) {
                ConstFold::Val(v) => ConstFold::Val(wrap_into(v, lo, hi)),
                ConstFold::TooBig => ConstFold::TooBig,
                ConstFold::NotConst => ConstFold::NotConst,
            }
        }
        _ => ConstFold::NotConst,
    }
}

/// Two's-complement wrap of `v` into the inclusive range `lo..=hi` — the
/// value SPEC X4 gives a constant conversion (`u8(300)` -> 44) and SPEC
/// X1/X2 give run-time arithmetic.
///
/// The width is recovered from the range itself, so this needs no second
/// table of type widths: `u8` is `0..=255`, so the span is 255 and the width
/// is 8 bits; `i64` is `-2^63..=2^63-1`, span `2^64-1`, width 64.
fn wrap_into(v: i128, lo: i128, hi: i128) -> i128 {
    let span = hi.wrapping_sub(lo);
    // `span + 1` is 2^bits; its trailing-zero count IS the width.
    let bits = (span as u128).wrapping_add(1).trailing_zeros();
    if bits == 0 || bits > 64 {
        return v;
    }
    let len = 1i128 << bits;
    let masked = v & (len - 1);
    if lo < 0 && (masked >> (bits - 1)) & 1 == 1 {
        masked - len
    } else {
        masked
    }
}

pub(crate) fn fold_bin(op: BinOp, l: ConstFold, r: ConstFold) -> ConstFold {
    use BinOp::*;
    // Division/shift-by-huge and `TooBig` interactions are handled per-op.
    match op {
        Add | Sub | Mul => {
            if matches!(l, ConstFold::NotConst) || matches!(r, ConstFold::NotConst) {
                return ConstFold::NotConst;
            }
            if matches!(l, ConstFold::TooBig) || matches!(r, ConstFold::TooBig) {
                return ConstFold::TooBig;
            }
            let (ConstFold::Val(a), ConstFold::Val(b)) = (l, r) else {
                return ConstFold::NotConst;
            };
            let v = match op {
                Add => a.checked_add(b),
                Sub => a.checked_sub(b),
                _ => a.checked_mul(b),
            };
            v.map(ConstFold::Val).unwrap_or(ConstFold::TooBig)
        }
        // Bitwise masking can shrink a `TooBig` value back into range —
        // give up (fail-open) rather than guess.
        BitAnd | BitOr | BitXor => {
            if matches!(l, ConstFold::TooBig) || matches!(r, ConstFold::TooBig) {
                return ConstFold::NotConst;
            }
            let (ConstFold::Val(a), ConstFold::Val(b)) = (l, r) else {
                return ConstFold::NotConst;
            };
            let v = match op {
                BitAnd => a & b,
                BitOr => a | b,
                _ => a ^ b,
            };
            ConstFold::Val(v)
        }
        Shl => {
            let (ConstFold::Val(a), ConstFold::Val(n)) = (l, r) else {
                // A provably-huge left operand stays huge when shifted by
                // a non-negative constant; anything else fails open.
                if matches!(l, ConstFold::TooBig)
                    && matches!(r, ConstFold::Val(n) if n >= 0)
                {
                    return ConstFold::TooBig;
                }
                return ConstFold::NotConst;
            };
            if n < 0 {
                // Negative counts are HYB-E6005's job at the shift site.
                return ConstFold::NotConst;
            }
            if a == 0 {
                return ConstFold::Val(0);
            }
            if n >= 128 {
                return ConstFold::TooBig;
            }
            let n = n as u32;
            let base = 1u128 << n;
            let Some(mag) = (a.unsigned_abs() as u128).checked_mul(base) else {
                return ConstFold::TooBig;
            };
            if mag > i128::MAX as u128 {
                return ConstFold::TooBig;
            }
            let mag = mag as i128;
            ConstFold::Val(if a < 0 { -mag } else { mag })
        }
        Shr => {
            let (ConstFold::Val(a), ConstFold::Val(n)) = (l, r) else {
                return ConstFold::NotConst;
            };
            if n < 0 {
                return ConstFold::NotConst;
            }
            if n >= 128 {
                // Arithmetic shift: non-negative → 0, negative → -1.
                return ConstFold::Val(if a < 0 { -1 } else { 0 });
            }
            ConstFold::Val(a >> (n as u32))
        }
        Div | Mod => {
            // `TooBig` operands give up: division shrinks and could land
            // back in range — never guess (fail-open, no false reject).
            if !matches!(l, ConstFold::Val(_)) || !matches!(r, ConstFold::Val(_)) {
                return ConstFold::NotConst;
            }
            let (ConstFold::Val(a), ConstFold::Val(b)) = (l, r) else {
                return ConstFold::NotConst;
            };
            if b == 0 {
                return ConstFold::NotConst; // HYB-E6001 owns `/ 0`
            }
            match op {
                Div => a.checked_div(b).map(ConstFold::Val).unwrap_or(ConstFold::TooBig),
                _ => a.checked_rem(b).map(ConstFold::Val).unwrap_or(ConstFold::NotConst),
            }
        }
        _ => ConstFold::NotConst,
    }
}

/// Convenience: fold to a value when the expression IS a constant in
/// range (used by the negative-shift check in emit_guard.rs).
pub(crate) fn const_int_value(e: &Expr) -> Option<i128> {
    match const_int_fold(e) {
        ConstFold::Val(v) => Some(v),
        _ => None,
    }
}

/// Fold a float constant expression (literals, unary `-`, parens) — the
/// `var x int = 2.5` truncation site (issue #201-B).
pub(crate) fn const_float_value(e: &Expr) -> Option<f64> {
    match unwrap_parens(e) {
        Expr::Lit {
            kind: LitKind::Float(s),
            ..
        } => s.replace('_', "").parse::<f64>().ok(),
        Expr::Unary {
            op: UnOp::Neg,
            operand,
            ..
        } => const_float_value(operand).map(|v| -v),
        _ => None,
    }
}

/// Fold a float constant expression, resolving identifiers via `consts`.
pub(crate) fn const_float_fold_with(
    e: &Expr,
    consts: &HashMap<String, &Expr>,
    depth: u32,
    st: Option<&FnState>,
) -> Option<f64> {
    match unwrap_parens(e) {
        Expr::Lit {
            kind: LitKind::Float(s),
            ..
        } => {
            let clean: String = s.chars().filter(|c| *c != '_').collect();
            clean.parse::<f64>().ok()
        }
        Expr::Lit {
            kind: LitKind::Int(s),
            ..
        } => {
            let clean: String = s.chars().filter(|c| *c != '_').collect();
            if let Some(ConstInt::Value(v)) = parse_int_lit(s) {
                Some(v as f64)
            } else if let Ok(f) = clean.parse::<f64>() {
                Some(f)
            } else {
                None
            }
        }
        Expr::Lit {
            kind: LitKind::Char(c),
            ..
        } => Some(*c as u32 as f64),
        Expr::Ident { name, .. } => {
            if let Some(st) = st {
                if st.env_lookup(name).is_some() {
                    return None;
                }
            }
            if depth < 16 {
                if let Some(v) = consts.get(name) {
                    return const_float_fold_with(v, consts, depth + 1, None);
                }
            }
            None
        }
        Expr::Unary {
            op: UnOp::Neg,
            operand,
            ..
        } => const_float_fold_with(operand, consts, depth + 1, st).map(|v| -v),
        Expr::Binary { op, lhs, rhs, .. } => {
            let l = const_float_fold_with(lhs, consts, depth + 1, st)?;
            let r = const_float_fold_with(rhs, consts, depth + 1, st)?;
            match op {
                BinOp::Add => Some(l + r),
                BinOp::Sub => Some(l - r),
                BinOp::Mul => Some(l * r),
                BinOp::Div => {
                    if r == 0.0 {
                        None
                    } else {
                        Some(l / r)
                    }
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// Returns true if `e` is an untyped float constant expression (contains at least one float literal or float const).
fn is_float_const_expr(
    e: &Expr,
    consts: &HashMap<String, &Expr>,
    depth: u32,
    st: Option<&FnState>,
) -> bool {
    match unwrap_parens(e) {
        Expr::Lit {
            kind: LitKind::Float(_),
            ..
        } => true,
        Expr::Lit { .. } => false,
        Expr::Ident { name, .. } => {
            if let Some(st) = st {
                if st.env_lookup(name).is_some() {
                    return false;
                }
            }
            if depth < 16 {
                if let Some(v) = consts.get(name) {
                    return is_float_const_expr(v, consts, depth + 1, None);
                }
            }
            false
        }
        Expr::Unary { operand, .. } => is_float_const_expr(operand, consts, depth + 1, st),
        Expr::Binary { lhs, rhs, .. } => {
            is_float_const_expr(lhs, consts, depth + 1, st)
                || is_float_const_expr(rhs, consts, depth + 1, st)
        }
        _ => false,
    }
}

/// Render a folded float as the nearest integer literal for the fix-it.
fn integer_of_float(f: f64) -> String {
    format!("{}", f.trunc() as i128)
}

pub(crate) fn parse_int_lit(s: &str) -> Option<ConstInt> {
    let clean: String = s.chars().filter(|c| *c != '_').collect();
    if clean.is_empty() {
        return None;
    }
    let s = clean.as_str();
    let (radix, digits) = if let Some(rest) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        (16, rest)
    } else if let Some(rest) = s.strip_prefix("0b").or_else(|| s.strip_prefix("0B")) {
        (2, rest)
    } else if let Some(rest) = s.strip_prefix("0o").or_else(|| s.strip_prefix("0O")) {
        (8, rest)
    } else if s.len() > 1 && s.starts_with('0') {
        if s.chars().any(|c| c == '8' || c == '9') {
            return None;
        }
        (8, s)
    } else {
        (10, s)
    };
    if digits.is_empty() {
        return None;
    }
    match i128::from_str_radix(digits, radix) {
        Ok(v) => Some(ConstInt::Value(v)),
        Err(err) => match err.kind() {
            std::num::IntErrorKind::PosOverflow => Some(ConstInt::PosOverflow),
            std::num::IntErrorKind::NegOverflow => Some(ConstInt::NegOverflow),
            _ => None,
        },
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn ranges_match_go_fixed_width() {
        assert_eq!(fixed_width_int_range("int8"), Some((-128, 127)));
        assert_eq!(fixed_width_int_range("byte"), Some((0, 255)));
        assert_eq!(fixed_width_int_range("u16"), Some((0, 65535)));
        assert!(fixed_width_int_range("string").is_none());
        assert!(fixed_width_int_range("float64").is_none());
    }

    #[test]
    fn parse_bases() {
        assert_eq!(parse_int_lit("300"), Some(ConstInt::Value(300)));
        assert_eq!(parse_int_lit("0x12C"), Some(ConstInt::Value(300)));
        assert_eq!(parse_int_lit("0b101"), Some(ConstInt::Value(5)));
        assert_eq!(parse_int_lit("0o10"), Some(ConstInt::Value(8)));
        assert_eq!(parse_int_lit("1_000"), Some(ConstInt::Value(1000)));
    }

    #[test]
    fn parse_overflow_and_target_ranges() {
        assert_eq!(
            parse_int_lit("340282366920938463463374607431768211457"),
            Some(ConstInt::PosOverflow)
        );
        assert_eq!(target_int_range(), (i64::MIN as i128, i64::MAX as i128));
        assert_eq!(target_uint_range(), (0, u64::MAX as i128));
    }
}
