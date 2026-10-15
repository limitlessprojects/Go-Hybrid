//! const_overflow.rs — constant initializers that overflow a declared
//! fixed-width integer type (issue #148).
//!
//! `var n int8 = 300` (and the file-level `const` twin) pass `hyb check`
//! with 0 diagnostics today, then fail `go vet` / `go build` with
//! `cannot use 300 as int8 value (overflows)`. The accepted-should-
//! emit-buildable-Go contract requires a checker diagnostic (HYB-E6002)
//! for that narrow constant case. Non-constant operands stay out of
//! scope — Go truncates at run time via an explicit conversion
//! (SPEC-LANGUAGE X4), and the conversion form `int8(x)` is unaffected.

use crate::model::{unwrap_parens, Checker};
use hy_diag::{Diagnostic, Span};
use hy_syntax::{CallArg, Expr, ForKind, InterpPart, LitKind, Place, PlaceKind, SelectArmKind, Stmt, TypeExpr, UnOp};

impl<'a> Checker<'a> {
    /// When `ty` is a fixed-width integer and `value` is an integer
    /// constant outside that type's range, emit HYB-E6002.
    pub(crate) fn check_const_overflow_init(&mut self, name: &str, ty: &TypeExpr, value: &Expr) {
        let TypeExpr::Named {
            name: ty_name,
            span: ty_span,
        } = ty
        else {
            return;
        };
        let Some((lo, hi)) = fixed_width_int_range(ty_name) else {
            return;
        };
        let Some(v) = const_int_value(value) else {
            return;
        };
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
                        SelectArmKind::Send { chan, value } => {
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
        // `[]byte("abc")` — a slice type-as-value called with one argument
        // (the standing type-as-value encoding; see hy-emit's conversion
        // branch for the emission half of issue #228).
        if let Expr::SliceLit {
            elem: Some(t),
            items,
            ..
        } = callee
        {
            if items.is_empty() {
                self.check_slice_conversion(t, arg, span);
                return;
            }
        }
        let Expr::Ident { name, .. } = callee else {
            return;
        };
        if !is_conversion_target(name) {
            return;
        }
        let Some(c) = conv_const(arg) else {
            return; // non-constant operand: run-time conversion (SPEC X4)
        };
        self.check_conv_against_target(name, &c, arg, span);
    }

    fn check_slice_conversion(&mut self, elem: &TypeExpr, arg: &Expr, span: Span) {
        let TypeExpr::Named { name: slice_elem, .. } = elem else {
            return; // []T with a non-named element + a run-time operand
        };
        match conv_const(arg) {
            Some(ConvConst::Str) => {
                // Go allows string -> []byte / []rune (and their aliases)
                // only; every other element type is a compile error.
                if !matches!(
                    slice_elem.as_str(),
                    "u8" | "byte" | "uint8" | "i32" | "int32" | "rune"
                ) {
                    self.emit_const_conversion_error(
                        span,
                        format!(
                            "a string constant cannot be converted to a `{}` slice (HYB-E6006)",
                            slice_elem
                        ),
                        vec![
                            format!(
                                "why: Go only allows string -> []byte / []rune conversions; `[]{}(\"…\")` fails to build (`cannot convert ... to type []{}`), so an accepted Hybrid program would emit unbuildable Go (issue #228)",
                                slice_elem, slice_elem
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
                        slice_elem
                    ),
                    vec![
                        format!(
                            "why: Go has no constant-to-slice conversion; `[]{}({})` fails to build (`invalid operation: cannot convert ...`), so an accepted Hybrid program would emit unbuildable Go (issue #228)",
                            slice_elem,
                            arg.render()
                        ),
                        "fix: build the slice explicitly (e.g. `[]{}{{{}}}` or `make([]{}, n)` with a variable length)".to_string(),
                    ],
                );
            }
            None => {} // run-time operand: element-type errors are Go's
        }
    }

    fn check_conv_against_target(&mut self, target: &str, c: &ConvConst, arg: &Expr, span: Span) {
        if let Some((lo, hi)) = fixed_width_int_range(target) {
            match c {
                ConvConst::Int(v) => {
                    if *v < lo || *v > hi {
                        if is_64bit_int_target(target) {
                            // 64-bit targets skip the emitter's X4 fold —
                            // nothing rewrites this, Go rejects it outright.
                            self.emit_const_conversion_error(
                                span,
                                format!(
                                    "constant `{}` overflows `{}` in a conversion (HYB-E6006)",
                                    arg.render(),
                                    target
                                ),
                                vec![
                                    format!(
                                        "why: Go rejects `{}({})` at compile time (`constant ... overflows {}`), so an accepted Hybrid program would emit unbuildable Go (issue #228)",
                                        target,
                                        arg.render(),
                                        target
                                    ),
                                    format!(
                                        "fix: use a constant in [{}, {}], or widen the target type",
                                        lo, hi
                                    ),
                                ],
                            );
                        }
                        // sub-64 targets: the emitter's SPEC X4 fold rewrites
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
                                target
                            ),
                            vec![
                                format!(
                                    "why: Go rejects `{}({})` at compile time (the code point does not fit `{}`), so an accepted Hybrid program would emit unbuildable Go (issue #228)",
                                    target,
                                    arg.render(),
                                    target
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
                                target
                            ),
                            vec![
                                format!(
                                    "why: Go only converts an untyped float constant to `{}` when it has no fractional part and fits the range (`cannot convert ...` / `overflows`), so an accepted Hybrid program would emit unbuildable Go (issue #228)",
                                    target
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
                            target
                        ),
                        vec![
                            format!(
                                "why: Go rejects `{}({})` at compile time (`constant ... overflows {}`), so an accepted Hybrid program would emit unbuildable Go (issue #228)",
                                target,
                                arg.render(),
                                target
                            ),
                            format!(
                                "fix: use a constant in [{}, {}], or widen the target type",
                                lo, hi
                            ),
                        ],
                    );
                }
                ConvConst::Str | ConvConst::Bool | ConvConst::Nil => {
                    self.emit_conv_kind_error(target, c, arg, span);
                }
            }
            return;
        }
        match target {
            // float targets: every i128 fits f64; f32's max (3.4e38)
            // exceeds i128's max (1.7e38), so integer/runes always fit.
            // Past-i128 literals carry their f64 reading and still have to
            // respect f32's max (`float32(1e40)` overflows f32 in Go).
            "f32" | "f64" | "float32" | "float64" => match c {
                ConvConst::Float(f) | ConvConst::Huge { f, .. } => {
                    let overflows = !f.is_finite()
                        || (is_f32_target(target) && f.abs() > f32::MAX as f64);
                    if overflows {
                        self.emit_const_conversion_error(
                            span,
                            format!(
                                "constant `{}` overflows `{}` in a conversion (HYB-E6006)",
                                arg.render(),
                                target
                            ),
                            vec![
                                format!(
                                    "why: Go rejects `{}({})` at compile time (`constant ... overflows {}`), so an accepted Hybrid program would emit unbuildable Go (issue #228)",
                                    target,
                                    arg.render(),
                                    target
                                ),
                                format!(
                                    "fix: use a constant within `{}` range, or widen the target type to `f64`",
                                    target
                                ),
                            ],
                        );
                    }
                }
                ConvConst::Str | ConvConst::Bool | ConvConst::Nil => {
                    self.emit_conv_kind_error(target, c, arg, span)
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
                            "integer constant `{}` does not fit the conversion to `string` (HYB-E6006)",
                            arg.render()
                        ),
                        vec![
                            format!(
                                "why: Go rejects `string({})` at compile time (`constant overflow` — string conversions accept integer constants below 2^512), so an accepted Hybrid program would emit unbuildable Go (issue #228)",
                                arg.render()
                            ),
                            "fix: use a smaller constant (below 2^512), or convert from a variable".to_string(),
                        ],
                    );
                }
                ConvConst::Float(_) | ConvConst::Bool | ConvConst::Nil => {
                    self.emit_conv_kind_error(target, c, arg, span)
                }
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
}

/// A literal-constant conversion operand (the literal-only scope shared with
/// E6001/E6002). Anything else — idents, binary ops, calls — is out of
/// scope: it is either run-time (SPEC X4) or another pass's business.
enum ConvConst {
    Int(i128),
    /// Integer literal past i128 (or past u128) — carries its `f64` reading
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

fn conv_const(e: &Expr) -> Option<ConvConst> {
    match unwrap_parens(e) {
        Expr::Lit { kind, .. } => Some(match kind {
            LitKind::Int(s) => match parse_int_lit(s) {
                Some(v) => ConvConst::Int(v),
                None => ConvConst::Huge {
                    f: huge_int_as_f64(s),
                    lt_512: lit_below_2_512(s),
                },
            },
            LitKind::Float(s) => ConvConst::Float(s.parse::<f64>().unwrap_or(f64::INFINITY)),
            LitKind::Char(c) => ConvConst::Char(*c as i32),
            LitKind::Str(_) | LitKind::RawStr(_) => ConvConst::Str,
            LitKind::Bool(_) => ConvConst::Bool,
            LitKind::Nil => ConvConst::Nil,
        }),
        Expr::Unary {
            op: UnOp::Neg,
            operand,
            ..
        } => match conv_const(unwrap_parens(operand))? {
            ConvConst::Int(v) => Some(match v.checked_neg() {
                Some(n) => ConvConst::Int(n),
                None => ConvConst::Huge {
                    // only reachable for `-i128::MIN` — |2^127| well under
                    // 2^512 and inside f64/f32 range both.
                    f: -(v as f64),
                    lt_512: true,
                },
            }),
            ConvConst::Float(f) => Some(ConvConst::Float(-f)),
            ConvConst::Huge { f, lt_512 } => Some(ConvConst::Huge { f: -f, lt_512 }),
            other => Some(other),
        },
        _ => None,
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
        || matches!(name, "f32" | "f64" | "float32" | "float64" | "string")
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

fn span_hint(span: Span) -> String {
    format!("{}:{}", span.start.line, span.start.col)
}

/// Inclusive representable range for a Hybrid fixed-width integer type
/// name (SPEC-LANGUAGE §11 / Go's typed-constant assignment rule).
/// Platform-word `int`/`uint`/`isize`/`usize` use the 64-bit LP64 ranges
/// the emitter targets (SPEC B9 / X7).
fn fixed_width_int_range(name: &str) -> Option<(i128, i128)> {
    Some(match name {
        "i8" | "int8" => (i8::MIN as i128, i8::MAX as i128),
        "u8" | "uint8" | "byte" => (0, u8::MAX as i128),
        "i16" | "int16" => (i16::MIN as i128, i16::MAX as i128),
        "u16" | "uint16" => (0, u16::MAX as i128),
        "i32" | "int32" | "rune" => (i32::MIN as i128, i32::MAX as i128),
        "u32" | "uint32" => (0, u32::MAX as i128),
        "i64" | "int64" | "int" | "isize" => (i64::MIN as i128, i64::MAX as i128),
        "u64" | "uint64" | "uint" | "usize" | "uintptr" => (0, u64::MAX as i128),
        _ => return None,
    })
}

/// Evaluate a pure integer constant expression the checker can see without
/// a full constant folder: int literals (any base the lexer keeps),
/// unary `-`, and parentheses. Anything else (idents, calls, binary ops)
/// is out of scope for HYB-E6002.
fn const_int_value(e: &Expr) -> Option<i128> {
    match unwrap_parens(e) {
        Expr::Lit {
            kind: LitKind::Int(s),
            ..
        } => parse_int_lit(s),
        Expr::Unary {
            op: UnOp::Neg,
            operand,
            ..
        } => {
            let v = const_int_value(operand)?;
            v.checked_neg()
        }
        _ => None,
    }
}

fn parse_int_lit(s: &str) -> Option<i128> {
    let s = s.as_bytes();
    if s.is_empty() {
        return None;
    }
    if s.len() >= 2 && s[0] == b'0' {
        match s[1].to_ascii_lowercase() {
            b'x' => return i128::from_str_radix(std::str::from_utf8(&s[2..]).ok()?, 16).ok(),
            b'b' => return i128::from_str_radix(std::str::from_utf8(&s[2..]).ok()?, 2).ok(),
            b'o' => return i128::from_str_radix(std::str::from_utf8(&s[2..]).ok()?, 8).ok(),
            _ => {}
        }
    }
    i128::from_str_radix(std::str::from_utf8(s).ok()?, 10).ok()
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
        assert_eq!(parse_int_lit("300"), Some(300));
        assert_eq!(parse_int_lit("0x12C"), Some(300));
        assert_eq!(parse_int_lit("0b101"), Some(5));
        assert_eq!(parse_int_lit("0o10"), Some(8));
    }
}
