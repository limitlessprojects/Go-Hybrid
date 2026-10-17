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

use crate::model::{unwrap_parens, Checker};
use hy_diag::{Diagnostic, Span};
use hy_syntax::{BinOp, Expr, LitKind, TypeExpr, UnOp};

impl<'a> Checker<'a> {
    /// When `ty` is a fixed-width integer and `value` is an integer
    /// constant (literal or folded constant expression) outside that
    /// type's range, or a float constant, emit HYB-E6002.
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
        match const_int_fold(value) {
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
    match unwrap_parens(e) {
        Expr::Lit {
            kind: LitKind::Int(s),
            ..
        } => match parse_int_lit(s) {
            Some(v) => ConstFold::Val(v),
            None => ConstFold::NotConst,
        },
        Expr::Unary {
            op: UnOp::Neg,
            operand,
            ..
        } => match const_int_fold(operand) {
            ConstFold::Val(v) => v.checked_neg().map(ConstFold::Val).unwrap_or(ConstFold::TooBig),
            ConstFold::NotConst => ConstFold::NotConst,
            ConstFold::TooBig => ConstFold::TooBig,
        },
        Expr::Binary { op, lhs, rhs, .. } => {
            let l = const_int_fold(lhs);
            let r = const_int_fold(rhs);
            fold_bin(*op, l, r)
        }
        _ => ConstFold::NotConst,
    }
}

fn fold_bin(op: BinOp, l: ConstFold, r: ConstFold) -> ConstFold {
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

/// Render a folded float as the nearest integer literal for the fix-it.
fn integer_of_float(f: f64) -> String {
    format!("{}", f.trunc() as i128)
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

// ---------------------------------------------------------------------------
// Issue #152 — relate expressions to declared shapes (call arity, return
// arity, struct literal fields/values, struct field access, map key / index)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrimFamily {
    Int,
    Float,
    Str,
    Bool,
}

pub(crate) fn primitive_family(ty_name: &str) -> Option<PrimFamily> {
    match ty_name {
        "int" | "int8" | "int16" | "int32" | "int64" | "uint" | "uint8" | "uint16" | "uint32"
        | "uint64" | "uintptr" | "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64"
        | "usize" | "isize" | "byte" | "rune" => Some(PrimFamily::Int),
        "float32" | "float64" | "f32" | "f64" => Some(PrimFamily::Float),
        "string" => Some(PrimFamily::Str),
        "bool" => Some(PrimFamily::Bool),
        _ => None,
    }
}

fn family_compatible(declared: PrimFamily, given: PrimFamily, is_const_lit: bool) -> bool {
    match declared {
        PrimFamily::Int => given == PrimFamily::Int,
        PrimFamily::Float => {
            given == PrimFamily::Float || (is_const_lit && given == PrimFamily::Int)
        }
        PrimFamily::Str => given == PrimFamily::Str,
        PrimFamily::Bool => given == PrimFamily::Bool,
    }
}

fn literal_family(e: &Expr) -> Option<(PrimFamily, &'static str)> {
    match unwrap_parens(e) {
        Expr::Lit { kind, .. } => match kind {
            LitKind::Int(_) | LitKind::Char(_) => Some((PrimFamily::Int, "an integer literal")),
            LitKind::Float(_) => Some((PrimFamily::Float, "a float literal")),
            LitKind::Str(_) | LitKind::RawStr(_) => Some((PrimFamily::Str, "a string literal")),
            LitKind::Bool(_) => Some((PrimFamily::Bool, "a bool literal")),
            LitKind::Nil => None,
        },
        Expr::Interp { .. } => Some((PrimFamily::Str, "an interpolated string")),
        Expr::Unary {
            op: UnOp::Neg,
            operand,
            ..
        } => literal_family(operand),
        Expr::Unary {
            op: UnOp::Not,
            operand,
            ..
        } => literal_family(operand).and_then(|(f, _)| {
            if f == PrimFamily::Bool {
                Some((PrimFamily::Bool, "a bool expression"))
            } else {
                None
            }
        }),
        _ => None,
    }
}

struct ShapeScopeEnv {
    scopes: Vec<std::collections::HashMap<String, Option<TypeExpr>>>,
}

impl ShapeScopeEnv {
    fn new() -> Self {
        Self {
            scopes: vec![std::collections::HashMap::new()],
        }
    }

    fn push(&mut self) {
        self.scopes.push(std::collections::HashMap::new());
    }

    fn pop(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    fn bind(&mut self, name: &str, ty: Option<TypeExpr>) {
        if name != "_" {
            if let Some(top) = self.scopes.last_mut() {
                top.insert(name.to_string(), ty);
            }
        }
    }

    fn is_shadowed(&self, name: &str) -> bool {
        self.scopes.iter().rev().any(|s| s.contains_key(name))
    }

    fn lookup(&self, name: &str) -> Option<&TypeExpr> {
        for s in self.scopes.iter().rev() {
            if let Some(entry) = s.get(name) {
                return entry.as_ref();
            }
        }
        None
    }
}

fn render_fn_sig(decl: &hy_syntax::FnDecl) -> String {
    let params: Vec<String> = decl
        .params
        .iter()
        .map(|p| format!("{} {}", p.name, p.ty.render()))
        .collect();
    format!("fn {}({})", decl.name, params.join(", "))
}

impl<'a> Checker<'a> {
    /// Entry point for Issue #152: checks call arity (HYB-E5002), return arity
    /// (HYB-E5003), struct-literal field names/duplicates (HYB-E5004),
    /// struct-literal field values (HYB-E5005), undefined struct field access
    /// (HYB-E5006), and map-key / index type mismatches (HYB-E5007).
    pub(crate) fn check_declared_shapes(&mut self) {
        let mut struct_type_params: std::collections::HashMap<
            String,
            std::collections::HashSet<String>,
        > = std::collections::HashMap::new();
        for d in &self.file.decls {
            if let hy_syntax::Decl::Struct(s) = d {
                let tps: std::collections::HashSet<String> =
                    s.type_params.iter().map(|tp| tp.name.clone()).collect();
                struct_type_params.insert(s.name.clone(), tps);
            }
        }

        let mut env = ShapeScopeEnv::new();
        for d in &self.file.decls {
            if let hy_syntax::Decl::Const(c) = d {
                let ty = c
                    .ty
                    .clone()
                    .or_else(|| self.infer_shape_type_expr(&env, &c.value));
                env.bind(&c.name, ty);
            }
        }

        for d in &self.file.decls {
            match d {
                hy_syntax::Decl::Const(c) => {
                    self.check_shapes_in_expr(&env, &struct_type_params, &c.value);
                }
                hy_syntax::Decl::Fn(f) => {
                    let Some(body) = &f.body else { continue };
                    env.push();
                    if let Some(r) = &f.receiver {
                        let rty = if r.is_pointer {
                            TypeExpr::Pointer {
                                to: Box::new(r.ty.clone()),
                                span: r.ty.span(),
                            }
                        } else {
                            r.ty.clone()
                        };
                        env.bind(&r.name, Some(rty));
                    }
                    for p in &f.params {
                        env.bind(&p.name, Some(p.ty.clone()));
                    }
                    for r in &f.results {
                        if let Some(rname) = &r.name {
                            env.bind(rname, Some(r.ty.clone()));
                        }
                    }
                    self.check_shapes_in_block(
                        &mut env,
                        &struct_type_params,
                        body,
                        Some((&f.name, &f.results, true)),
                    );
                    env.pop();
                }
                _ => {}
            }
        }
    }

    fn resolve_alias_type<'t>(&'t self, ty: &'t TypeExpr) -> &'t TypeExpr {
        let mut cur = ty;
        for _ in 0..8 {
            if let TypeExpr::Named { name, .. } = cur {
                if let Some(next) = self.type_aliases.get(name.as_str()) {
                    cur = next;
                    continue;
                }
            }
            break;
        }
        cur
    }

    fn base_struct_name_of_type(&self, ty: &TypeExpr) -> Option<String> {
        let resolved = self.resolve_alias_type(ty);
        match resolved {
            TypeExpr::Pointer { to, .. } => self.base_struct_name_of_type(to),
            TypeExpr::Named { name, .. } => {
                let base = name.split('[').next().unwrap_or(name.as_str()).trim();
                if base.contains('.') || base.is_empty() {
                    None
                } else {
                    Some(base.to_string())
                }
            }
            _ => None,
        }
    }

    fn infer_shape_type_expr(&self, env: &ShapeScopeEnv, e: &Expr) -> Option<TypeExpr> {
        let dummy = e.span();
        match unwrap_parens(e) {
            Expr::Lit { kind, span } => match kind {
                LitKind::Int(_) | LitKind::Char(_) => Some(TypeExpr::Named {
                    name: "int".to_string(),
                    span: *span,
                }),
                LitKind::Float(_) => Some(TypeExpr::Named {
                    name: "float64".to_string(),
                    span: *span,
                }),
                LitKind::Str(_) | LitKind::RawStr(_) => Some(TypeExpr::Named {
                    name: "string".to_string(),
                    span: *span,
                }),
                LitKind::Bool(_) => Some(TypeExpr::Named {
                    name: "bool".to_string(),
                    span: *span,
                }),
                LitKind::Nil => None,
            },
            Expr::Interp { span, .. } => Some(TypeExpr::Named {
                name: "string".to_string(),
                span: *span,
            }),
            Expr::Ident { name, .. } => env.lookup(name).cloned(),
            Expr::Move { operand, .. } => self.infer_shape_type_expr(env, operand),
            Expr::AddrOf { operand, span }
            | Expr::Unary {
                op: UnOp::Addr,
                operand,
                span,
            } => self
                .infer_shape_type_expr(env, operand)
                .map(|inner| TypeExpr::Pointer {
                    to: Box::new(inner),
                    span: *span,
                }),
            Expr::Unary {
                op: UnOp::Neg,
                operand,
                ..
            } => self.infer_shape_type_expr(env, operand),
            Expr::Unary {
                op: UnOp::Not,
                span,
                ..
            } => Some(TypeExpr::Named {
                name: "bool".to_string(),
                span: *span,
            }),
            Expr::StructLit { ty: Some(t), .. } => Some(t.clone()),
            Expr::MapLit {
                key, value, span, ..
            } => Some(TypeExpr::Map {
                key: key.clone(),
                value: value.clone(),
                span: *span,
            }),
            Expr::SliceLit {
                elem: Some(el),
                span,
                ..
            } => Some(TypeExpr::Slice {
                elem: el.clone(),
                span: *span,
            }),
            Expr::Field { base, name, .. } => {
                let bty = self.infer_shape_type_expr(env, base)?;
                let sname = self.base_struct_name_of_type(&bty)?;
                let fields = self.structs.get(&sname)?;
                fields
                    .iter()
                    .find(|(fname, _)| fname == name)
                    .map(|(_, fty)| fty.clone())
            }
            Expr::Index { base, .. } => {
                let bty = self.infer_shape_type_expr(env, base)?;
                match self.resolve_alias_type(&bty) {
                    TypeExpr::Slice { elem, .. } | TypeExpr::Array { elem, .. } => {
                        Some((**elem).clone())
                    }
                    TypeExpr::Map { value, .. } => Some((**value).clone()),
                    TypeExpr::Named { name, .. } if name == "string" => Some(TypeExpr::Named {
                        name: "byte".to_string(),
                        span: dummy,
                    }),
                    _ => None,
                }
            }
            Expr::Receive { chan, .. } => {
                let cty = self.infer_shape_type_expr(env, chan)?;
                match self.resolve_alias_type(&cty) {
                    TypeExpr::Chan { elem, .. } => Some((**elem).clone()),
                    _ => None,
                }
            }
            Expr::Call { callee, args, span } => {
                if let Expr::Ident { name, .. } = unwrap_parens(callee) {
                    if !env.is_shadowed(name) {
                        match name.as_str() {
                            "make" => {
                                return args.first().and_then(|a| {
                                    if let Expr::Ident { name: tname, .. } = &a.value {
                                        Some(TypeExpr::Named {
                                            name: tname.clone(),
                                            span: *span,
                                        })
                                    } else {
                                        self.infer_shape_type_expr(env, &a.value)
                                    }
                                })
                            }
                            "len" | "cap" => {
                                return Some(TypeExpr::Named {
                                    name: "int".to_string(),
                                    span: *span,
                                })
                            }
                            _ => {}
                        }
                        if let Some(fdecl) = self.fns.get(name.as_str()) {
                            if fdecl.results.len() == 1 && fdecl.type_params.is_empty() {
                                return Some(fdecl.results[0].ty.clone());
                            }
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn infer_place_type_expr(
        &self,
        env: &ShapeScopeEnv,
        place: &hy_syntax::Place,
    ) -> Option<TypeExpr> {
        match &place.kind {
            hy_syntax::PlaceKind::Ident(name) => env.lookup(name).cloned(),
            hy_syntax::PlaceKind::AddrOf { base } => {
                self.infer_place_type_expr(env, base)
                    .map(|inner| TypeExpr::Pointer {
                        to: Box::new(inner),
                        span: place.span,
                    })
            }
            hy_syntax::PlaceKind::Field { base, name } => {
                let bty = self.infer_place_type_expr(env, base)?;
                let sname = self.base_struct_name_of_type(&bty)?;
                let fields = self.structs.get(&sname)?;
                fields
                    .iter()
                    .find(|(fname, _)| fname == name)
                    .map(|(_, fty)| fty.clone())
            }
            hy_syntax::PlaceKind::Index { base, .. } => {
                let bty = self.infer_place_type_expr(env, base)?;
                match self.resolve_alias_type(&bty) {
                    TypeExpr::Slice { elem, .. } | TypeExpr::Array { elem, .. } => {
                        Some((**elem).clone())
                    }
                    TypeExpr::Map { value, .. } => Some((**value).clone()),
                    _ => None,
                }
            }
        }
    }

    fn call_result_count(&self, env: &ShapeScopeEnv, e: &Expr) -> Option<usize> {
        let Expr::Call { callee, .. } = unwrap_parens(e) else {
            return None;
        };
        match unwrap_parens(callee) {
            Expr::Ident { name, .. } => {
                if let Some(ty) = env.lookup(name) {
                    if let TypeExpr::Fn { results, .. } = self.resolve_alias_type(ty) {
                        return Some(results.len());
                    }
                    return None;
                }
                self.fns.get(name.as_str()).map(|f| f.results.len())
            }
            Expr::Field { base, name, .. } => {
                let bty = self.infer_shape_type_expr(env, base)?;
                let sname = self.base_struct_name_of_type(&bty)?;
                self.method_decls
                    .get(&(sname, name.clone()))
                    .map(|m| m.results.len())
            }
            _ => None,
        }
    }

    fn check_shapes_in_block(
        &mut self,
        env: &mut ShapeScopeEnv,
        struct_type_params: &std::collections::HashMap<String, std::collections::HashSet<String>>,
        block: &hy_syntax::Block,
        ret_ctx: Option<(&str, &[hy_syntax::FnResult], bool)>,
    ) {
        for stmt in &block.stmts {
            self.check_shapes_in_stmt(env, struct_type_params, stmt, ret_ctx);
        }
    }

    fn check_shapes_in_stmt(
        &mut self,
        env: &mut ShapeScopeEnv,
        struct_type_params: &std::collections::HashMap<String, std::collections::HashSet<String>>,
        stmt: &hy_syntax::Stmt,
        ret_ctx: Option<(&str, &[hy_syntax::FnResult], bool)>,
    ) {
        use hy_syntax::{ForKind, MatchPattern, SelectArmKind, Stmt};
        match stmt {
            Stmt::Local(l) => {
                for v in &l.values {
                    self.check_shapes_in_expr(env, struct_type_params, v);
                }
                if let Some(decl_ty) = &l.ty {
                    for name in &l.names {
                        env.bind(name, Some(decl_ty.clone()));
                    }
                } else if l.values.len() == l.names.len() {
                    for (name, v) in l.names.iter().zip(l.values.iter()) {
                        let inferred = self.infer_shape_type_expr(env, v);
                        env.bind(name, inferred);
                    }
                } else if l.values.len() == 1 && l.names.len() > 1 {
                    let mut bound = false;
                    if let Expr::Call { callee, .. } = unwrap_parens(&l.values[0]) {
                        if let Expr::Ident { name: cname, .. } = unwrap_parens(callee) {
                            if !env.is_shadowed(cname) {
                                if let Some(fdecl) = self.fns.get(cname.as_str()) {
                                    if fdecl.results.len() == l.names.len()
                                        && fdecl.type_params.is_empty()
                                    {
                                        for (n, r) in l.names.iter().zip(fdecl.results.iter()) {
                                            env.bind(n, Some(r.ty.clone()));
                                        }
                                        bound = true;
                                    }
                                }
                            }
                        }
                    }
                    if !bound {
                        for n in &l.names {
                            env.bind(n, None);
                        }
                    }
                } else {
                    for n in &l.names {
                        env.bind(n, None);
                    }
                }
            }
            Stmt::Assign {
                targets, values, ..
            } => {
                for t in targets {
                    self.check_shapes_in_place(env, struct_type_params, t);
                }
                for v in values {
                    self.check_shapes_in_expr(env, struct_type_params, v);
                }
            }
            Stmt::Expr { expr, .. } => {
                self.check_shapes_in_expr(env, struct_type_params, expr);
            }
            Stmt::Send { chan, value, .. } => {
                self.check_shapes_in_expr(env, struct_type_params, chan);
                self.check_shapes_in_expr(env, struct_type_params, value);
            }
            Stmt::Defer { call, .. } => {
                self.check_shapes_in_expr(env, struct_type_params, call);
            }
            Stmt::Task { callee, args, span } => {
                self.check_call_arity_site(env, callee, args, *span);
                self.check_shapes_in_expr(env, struct_type_params, callee);
                for a in args {
                    self.check_shapes_in_expr(env, struct_type_params, &a.value);
                }
            }
            Stmt::Return { values, span } => {
                for v in values {
                    self.check_shapes_in_expr(env, struct_type_params, v);
                }
                if let Some((fn_name, results, enforce_when_empty)) = ret_ctx {
                    self.check_return_arity_site(
                        env,
                        fn_name,
                        results,
                        enforce_when_empty,
                        values,
                        *span,
                    );
                }
            }
            Stmt::If {
                init,
                cond,
                then,
                otherwise,
                ..
            } => {
                env.push();
                if let Some(init_stmt) = init {
                    self.check_shapes_in_stmt(env, struct_type_params, init_stmt, ret_ctx);
                }
                self.check_shapes_in_expr(env, struct_type_params, cond);
                env.push();
                self.check_shapes_in_block(env, struct_type_params, then, ret_ctx);
                env.pop();
                if let Some(else_stmt) = otherwise {
                    env.push();
                    self.check_shapes_in_stmt(env, struct_type_params, else_stmt, ret_ctx);
                    env.pop();
                }
                env.pop();
            }
            Stmt::For { kind, body, .. } => {
                env.push();
                match kind {
                    ForKind::Infinite => {}
                    ForKind::While { cond } => {
                        self.check_shapes_in_expr(env, struct_type_params, cond);
                    }
                    ForKind::Range { var, lo, hi, .. } => {
                        self.check_shapes_in_expr(env, struct_type_params, lo);
                        self.check_shapes_in_expr(env, struct_type_params, hi);
                        env.bind(
                            var,
                            Some(TypeExpr::Named {
                                name: "int".to_string(),
                                span: lo.span(),
                            }),
                        );
                    }
                    ForKind::Iterate { vars, expr } => {
                        self.check_shapes_in_expr(env, struct_type_params, expr);
                        let it_ty = self.infer_shape_type_expr(env, expr);
                        for v in vars {
                            env.bind(v, None);
                        }
                        if let Some(t) = it_ty {
                            match self.resolve_alias_type(&t) {
                                TypeExpr::Map { key, value, .. } => {
                                    if let Some(v0) = vars.first() {
                                        env.bind(v0, Some((**key).clone()));
                                    }
                                    if let Some(v1) = vars.get(1) {
                                        env.bind(v1, Some((**value).clone()));
                                    }
                                }
                                TypeExpr::Slice { elem, .. } | TypeExpr::Array { elem, .. } => {
                                    if vars.len() == 1 {
                                        env.bind(&vars[0], Some((**elem).clone()));
                                    } else if vars.len() >= 2 {
                                        env.bind(
                                            &vars[0],
                                            Some(TypeExpr::Named {
                                                name: "int".to_string(),
                                                span: expr.span(),
                                            }),
                                        );
                                        env.bind(&vars[1], Some((**elem).clone()));
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                self.check_shapes_in_block(env, struct_type_params, body, ret_ctx);
                env.pop();
            }
            Stmt::Managed { body, .. } | Stmt::Block { block: body, .. } => {
                env.push();
                self.check_shapes_in_block(env, struct_type_params, body, ret_ctx);
                env.pop();
            }
            Stmt::Match { subject, arms, .. } => {
                self.check_shapes_in_expr(env, struct_type_params, subject);
                let subj_ty = self.infer_shape_type_expr(env, subject);
                for arm in arms {
                    env.push();
                    if let MatchPattern::Path {
                        segments,
                        binding: Some(bname),
                    } = &arm.pattern
                    {
                        let mut payload_ty = None;
                        if segments.len() == 1 && segments[0] == "some" {
                            if let Some(st) = &subj_ty {
                                if let TypeExpr::Option { elem, .. } = self.resolve_alias_type(st) {
                                    payload_ty = Some((**elem).clone());
                                }
                            }
                        }
                        env.bind(bname, payload_ty);
                    }
                    if let Some(g) = &arm.guard {
                        self.check_shapes_in_expr(env, struct_type_params, g);
                    }
                    self.check_shapes_in_block(env, struct_type_params, &arm.body, ret_ctx);
                    env.pop();
                }
            }
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    env.push();
                    match &arm.kind {
                        SelectArmKind::Send { chan, value } => {
                            self.check_shapes_in_expr(env, struct_type_params, chan);
                            self.check_shapes_in_expr(env, struct_type_params, value);
                        }
                        SelectArmKind::Recv { names, chan } => {
                            self.check_shapes_in_expr(env, struct_type_params, chan);
                            let elem_ty = self.infer_shape_type_expr(env, chan).and_then(|t| {
                                match self.resolve_alias_type(&t) {
                                    TypeExpr::Chan { elem, .. } => Some((**elem).clone()),
                                    _ => None,
                                }
                            });
                            if let Some(n0) = names.first() {
                                env.bind(n0, elem_ty);
                            }
                            for n in names.iter().skip(1) {
                                env.bind(
                                    n,
                                    Some(TypeExpr::Named {
                                        name: "bool".to_string(),
                                        span: chan.span(),
                                    }),
                                );
                            }
                        }
                        SelectArmKind::Default => {}
                    }
                    self.check_shapes_in_block(env, struct_type_params, &arm.body, ret_ctx);
                    env.pop();
                }
            }
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
        }
    }

    fn check_shapes_in_place(
        &mut self,
        env: &ShapeScopeEnv,
        struct_type_params: &std::collections::HashMap<String, std::collections::HashSet<String>>,
        place: &hy_syntax::Place,
    ) {
        match &place.kind {
            hy_syntax::PlaceKind::Ident(_) => {}
            hy_syntax::PlaceKind::AddrOf { base } => {
                self.check_shapes_in_place(env, struct_type_params, base);
            }
            hy_syntax::PlaceKind::Field { base, name } => {
                self.check_shapes_in_place(env, struct_type_params, base);
                if let Some(bty) = self.infer_place_type_expr(env, base) {
                    self.check_struct_field_access(&bty, &base.render(), name, place.span);
                }
            }
            hy_syntax::PlaceKind::Index { base, index } => {
                self.check_shapes_in_place(env, struct_type_params, base);
                self.check_shapes_in_expr(env, struct_type_params, index);
                let bty = self.infer_place_type_expr(env, base);
                self.check_index_key_or_float(env, bty.as_ref(), index, place.span);
            }
        }
    }

    fn check_shapes_in_expr(
        &mut self,
        env: &ShapeScopeEnv,
        struct_type_params: &std::collections::HashMap<String, std::collections::HashSet<String>>,
        expr: &Expr,
    ) {
        match expr {
            Expr::Ident { .. } | Expr::Lit { .. } => {}
            Expr::Interp { parts, .. } => {
                for p in parts {
                    if let hy_syntax::InterpPart::Expr(inner) = p {
                        self.check_shapes_in_expr(env, struct_type_params, inner);
                    }
                }
            }
            Expr::Binary { lhs, rhs, .. } => {
                self.check_shapes_in_expr(env, struct_type_params, lhs);
                self.check_shapes_in_expr(env, struct_type_params, rhs);
            }
            Expr::Unary { operand, .. }
            | Expr::AddrOf { operand, .. }
            | Expr::Move { operand, .. }
            | Expr::Try { operand, .. }
            | Expr::Receive { chan: operand, .. }
            | Expr::Paren { inner: operand, .. } => {
                self.check_shapes_in_expr(env, struct_type_params, operand);
            }
            Expr::Slice { base, lo, hi, .. } => {
                self.check_shapes_in_expr(env, struct_type_params, base);
                if let Some(l) = lo {
                    self.check_shapes_in_expr(env, struct_type_params, l);
                }
                if let Some(h) = hi {
                    self.check_shapes_in_expr(env, struct_type_params, h);
                }
            }
            Expr::Index {
                base, index, span, ..
            } => {
                self.check_shapes_in_expr(env, struct_type_params, base);
                self.check_shapes_in_expr(env, struct_type_params, index);
                let bty = self.infer_shape_type_expr(env, base);
                self.check_index_key_or_float(env, bty.as_ref(), index, *span);
            }
            Expr::Field {
                base, name, span, ..
            } => {
                self.check_shapes_in_expr(env, struct_type_params, base);
                if let Expr::Ident { name: root, .. } = unwrap_parens(base) {
                    if !env.is_shadowed(root)
                        && (self.packages.contains(root)
                            || crate::foreign::BUILTIN_PACKAGES.contains(&root.as_str())
                            || self.enums.contains_key(root)
                            || self.structs.contains_key(root)
                            || self.type_aliases.contains_key(root))
                    {
                        return;
                    }
                }
                if let Some(bty) = self.infer_shape_type_expr(env, base) {
                    self.check_struct_field_access(&bty, &base.render(), name, *span);
                }
            }
            Expr::Call {
                callee, args, span, ..
            } => {
                self.check_call_arity_site(env, callee, args, *span);
                self.check_shapes_in_expr(env, struct_type_params, callee);
                for a in args {
                    self.check_shapes_in_expr(env, struct_type_params, &a.value);
                }
            }
            Expr::StructLit {
                ty, fields, span, ..
            } => {
                for f in fields {
                    self.check_shapes_in_expr(env, struct_type_params, &f.value);
                }
                self.check_struct_lit_site(env, struct_type_params, ty, fields, *span);
            }
            Expr::SliceLit { items, .. } => {
                for it in items {
                    self.check_shapes_in_expr(env, struct_type_params, it);
                }
            }
            Expr::MapLit { key, entries, .. } => {
                for (k, v) in entries {
                    self.check_shapes_in_expr(env, struct_type_params, k);
                    self.check_shapes_in_expr(env, struct_type_params, v);
                    self.check_map_key_expr(env, key, k, k.span());
                }
            }
            Expr::FnLit {
                params,
                results,
                body,
                ..
            } => {
                let mut cenv = ShapeScopeEnv {
                    scopes: env.scopes.clone(),
                };
                cenv.push();
                for p in params {
                    cenv.bind(&p.name, Some(p.ty.clone()));
                }
                for r in results {
                    if let Some(rname) = &r.name {
                        cenv.bind(rname, Some(r.ty.clone()));
                    }
                }
                self.check_shapes_in_block(
                    &mut cenv,
                    struct_type_params,
                    body,
                    Some(("closure", results, !results.is_empty())),
                );
            }
        }
    }

    fn check_call_arity_site(
        &mut self,
        env: &ShapeScopeEnv,
        callee: &Expr,
        args: &[hy_syntax::CallArg],
        span: Span,
    ) {
        if args.iter().any(|a| a.spread) {
            return;
        }
        let mut given = args.len();
        if args.len() == 1 {
            if let Some(forwarded) = self.call_result_count(env, &args[0].value) {
                if forwarded > 1 {
                    given = forwarded;
                }
            } else if matches!(unwrap_parens(&args[0].value), Expr::Call { .. }) {
                return;
            }
        }

        match unwrap_parens(callee) {
            Expr::Ident { name, .. } => {
                if env.is_shadowed(name) {
                    if let Some(ty) = env.lookup(name) {
                        if let TypeExpr::Fn { params, .. } = self.resolve_alias_type(ty) {
                            let declared = params.len();
                            if given != declared {
                                let dir = if given < declared {
                                    "not enough arguments"
                                } else {
                                    "too many arguments"
                                };
                                self.diags.push(
                                    Diagnostic::error(
                                        "HYB-E5002",
                                        self.path.clone(),
                                        span,
                                        format!(
                                            "call to `{}` passes {} argument(s) but the function declares {} ({} in call to `{}`) (HYB-E5002)",
                                            name, given, declared, dir, name
                                        ),
                                    ),
                                );
                            }
                        }
                    }
                    return;
                }
                let Some(decl) = self.fns.get(name.as_str()).copied() else {
                    return;
                };
                let declared = decl.params.len();
                if given != declared {
                    let dir = if given < declared {
                        "not enough arguments"
                    } else {
                        "too many arguments"
                    };
                    self.diags.push(
                        Diagnostic::error(
                            "HYB-E5002",
                            self.path.clone(),
                            span,
                            format!(
                                "call to `{}` passes {} argument(s) but the function declares {} (HYB-E5002)",
                                name, given, declared
                            ),
                        )
                        .with_notes(vec![
                            format!("`{}` is declared as `{}`", name, render_fn_sig(decl)),
                            format!("the emitted Go cannot build: `{} in call to {}`", dir, name),
                        ]),
                    );
                }
            }
            Expr::Field { base, name, .. } => {
                let Some(bty) = self.infer_shape_type_expr(env, base) else {
                    return;
                };
                let Some(sname) = self.base_struct_name_of_type(&bty) else {
                    return;
                };
                let Some(decl) = self.method_decls.get(&(sname, name.clone())).copied() else {
                    return;
                };
                let declared = decl.params.len();
                if given != declared {
                    let dir = if given < declared {
                        "not enough arguments"
                    } else {
                        "too many arguments"
                    };
                    self.diags.push(
                        Diagnostic::error(
                            "HYB-E5002",
                            self.path.clone(),
                            span,
                            format!(
                                "call to `{}` passes {} argument(s) but the method declares {} (HYB-E5002)",
                                name, given, declared
                            ),
                        )
                        .with_notes(vec![
                            format!("`{}` is declared as `{}`", name, render_fn_sig(decl)),
                            format!("the emitted Go cannot build: `{} in call to {}`", dir, name),
                        ]),
                    );
                }
            }
            _ => {}
        }
    }

    fn check_return_arity_site(
        &mut self,
        env: &ShapeScopeEnv,
        fn_name: &str,
        results: &[hy_syntax::FnResult],
        enforce_when_empty: bool,
        values: &[Expr],
        span: Span,
    ) {
        if results.is_empty() && !enforce_when_empty {
            return;
        }
        let has_named = results.iter().any(|r| r.name.is_some());
        if values.is_empty() && (results.is_empty() || has_named) {
            return;
        }
        let declared = results.len();
        let mut given = values.len();
        if values.len() == 1 {
            if let Some(forwarded) = self.call_result_count(env, &values[0]) {
                if forwarded > 1 {
                    given = forwarded;
                }
            } else if matches!(unwrap_parens(&values[0]), Expr::Call { .. }) && declared > 1 {
                return;
            }
        }
        let ends_in_error = results
            .last()
            .is_some_and(|r| matches!(&r.ty, TypeExpr::Named { name, .. } if name == "error"));
        if ends_in_error && given + 1 == declared {
            // SPEC E3 ok-return sugar (corpus 13): `return v` in `(T, error)`.
            return;
        }
        if given != declared {
            let dir = if given < declared {
                "not enough return values"
            } else {
                "too many return values"
            };
            self.diags.push(
                Diagnostic::error(
                    "HYB-E5003",
                    self.path.clone(),
                    span,
                    format!(
                        "`return` produces {} value(s) but `{}` declares {} result(s) (HYB-E5003)",
                        given, fn_name, declared
                    ),
                )
                .with_notes(vec![format!("the emitted Go cannot build: `{}`", dir)]),
            );
        }
    }

    fn check_struct_lit_site(
        &mut self,
        env: &ShapeScopeEnv,
        struct_type_params: &std::collections::HashMap<String, std::collections::HashSet<String>>,
        ty: &Option<TypeExpr>,
        fields: &[hy_syntax::FieldInit],
        span: Span,
    ) {
        let Some(ty_expr) = ty.as_ref() else {
            return;
        };
        let Some(sname) = self.base_struct_name_of_type(ty_expr) else {
            return;
        };
        let Some(declared) = self.structs.get(&sname).cloned() else {
            return;
        };
        if declared.is_empty() {
            return;
        }
        let tps = struct_type_params.get(&sname);

        // 1. Value type mismatch (HYB-E5005)
        for f in fields {
            let Some((_, dty)) = declared.iter().find(|(n, _)| n == &f.name) else {
                continue;
            };
            let TypeExpr::Named { name: dname, .. } = self.resolve_alias_type(dty) else {
                continue;
            };
            if tps.is_some_and(|set| set.contains(dname.as_str())) {
                continue;
            }
            let Some(want) = primitive_family(dname) else {
                continue;
            };
            let given_info = if let Some((fam, desc)) = literal_family(&f.value) {
                Some((fam, desc.to_string(), true))
            } else if let Some(vty) = self.infer_shape_type_expr(env, &f.value) {
                if let TypeExpr::Named { name: vname, .. } = self.resolve_alias_type(&vty) {
                    primitive_family(vname).map(|fam| (fam, format!("of type `{}`", vname), false))
                } else {
                    None
                }
            } else {
                None
            };
            if let Some((got, desc, is_const_lit)) = given_info {
                if !family_compatible(want, got, is_const_lit) {
                    self.diags.push(
                        Diagnostic::error(
                            "HYB-E5005",
                            self.path.clone(),
                            span,
                            format!(
                                "field `{}` of `{}` is declared `{}` but the literal is {} (HYB-E5005)",
                                f.name, sname, dname, desc
                            ),
                        )
                        .with_notes(vec![
                            "the emitted Go cannot build: `cannot use ... as ... value in struct literal`"
                                .to_string(),
                        ]),
                    );
                    break;
                }
            }
        }

        // 2. Field existence & duplicate check (HYB-E5004)
        let mut seen: Vec<&str> = Vec::new();
        for f in fields {
            if declared.iter().any(|(n, _)| n == &f.name) {
                if seen.contains(&f.name.as_str()) {
                    self.diags.push(
                        Diagnostic::error(
                            "HYB-E5004",
                            self.path.clone(),
                            span,
                            format!(
                                "struct literal for `{}` gives field `{}` more than once (duplicate field name `{}` in struct literal) (HYB-E5004)",
                                sname, f.name, f.name
                            ),
                        )
                        .with_notes(vec![
                            "the emitted Go cannot build: `duplicate field ... in struct literal`"
                                .to_string(),
                        ]),
                    );
                    break;
                }
                seen.push(f.name.as_str());
            } else {
                let decl_names = declared
                    .iter()
                    .map(|(n, _)| n.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                self.diags.push(
                    Diagnostic::error(
                        "HYB-E5004",
                        self.path.clone(),
                        span,
                        format!(
                            "struct literal for `{}` names field `{}`, which the struct does not declare (HYB-E5004)",
                            sname, f.name
                        ),
                    )
                    .with_notes(vec![
                        format!("`{}` declares: {}", sname, decl_names),
                        "the emitted Go cannot build: `unknown field ... in struct literal`"
                            .to_string(),
                    ]),
                );
                break;
            }
        }
    }

    fn check_struct_field_access(
        &mut self,
        base_ty: &TypeExpr,
        base_render: &str,
        field: &str,
        span: Span,
    ) {
        let Some(sname) = self.base_struct_name_of_type(base_ty) else {
            return;
        };
        let Some(declared) = self.structs.get(&sname) else {
            return;
        };
        if declared.is_empty() {
            return;
        }
        let has_field = declared.iter().any(|(n, _)| n == field);
        let has_method = self
            .method_decls
            .contains_key(&(sname.clone(), field.to_string()));
        if !has_field && !has_method {
            let decl_names = declared
                .iter()
                .map(|(n, _)| n.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            self.diags.push(
                Diagnostic::error(
                    "HYB-E5006",
                    self.path.clone(),
                    span,
                    format!(
                        "`{}.{}` undefined (type `{}` has no field or method `{}`) (HYB-E5006)",
                        base_render, field, sname, field
                    ),
                )
                .with_notes(vec![
                    format!("`{}` declares fields: {}", sname, decl_names),
                    format!(
                        "the emitted Go cannot build: `{}.{} undefined (type {} has no field or method {})`",
                        base_render, field, sname, field
                    ),
                ]),
            );
        }
    }

    fn check_index_key_or_float(
        &mut self,
        env: &ShapeScopeEnv,
        base_ty: Option<&TypeExpr>,
        index: &Expr,
        span: Span,
    ) {
        if let Some(bty) = base_ty {
            let resolved = self.resolve_alias_type(bty).clone();
            match &resolved {
                TypeExpr::Map { key, .. } => {
                    self.check_map_key_expr(env, key, index, span);
                    return;
                }
                TypeExpr::Slice { .. } | TypeExpr::Array { .. } => {
                    if const_float_value(index).is_some() {
                        self.diags.push(
                            Diagnostic::error(
                                "HYB-E5007",
                                self.path.clone(),
                                span,
                                format!(
                                    "index `{}` is a float constant (`{}` truncated to int) — slice and array indices must be integers (HYB-E5007)",
                                    index.render(),
                                    index.render()
                                ),
                            )
                            .with_notes(vec![
                                "the emitted Go cannot build: `constant ... truncated to int`"
                                    .to_string(),
                            ]),
                        );
                    }
                    return;
                }
                TypeExpr::Named { name, .. } if name == "string" => {
                    if const_float_value(index).is_some() {
                        self.diags.push(
                            Diagnostic::error(
                                "HYB-E5007",
                                self.path.clone(),
                                span,
                                format!(
                                    "index `{}` is a float constant (`{}` truncated to int) — string indices must be integers (HYB-E5007)",
                                    index.render(),
                                    index.render()
                                ),
                            )
                            .with_notes(vec![
                                "the emitted Go cannot build: `constant ... truncated to int`"
                                    .to_string(),
                            ]),
                        );
                    }
                    return;
                }
                _ => {}
            }
        }
    }

    fn check_map_key_expr(
        &mut self,
        env: &ShapeScopeEnv,
        key_ty: &TypeExpr,
        key_expr: &Expr,
        span: Span,
    ) {
        let TypeExpr::Named { name: kname, .. } = self.resolve_alias_type(key_ty) else {
            return;
        };
        let Some(want) = primitive_family(kname) else {
            return;
        };
        let given_info = if let Some((fam, desc)) = literal_family(key_expr) {
            Some((fam, desc.to_string(), true))
        } else if let Some(vty) = self.infer_shape_type_expr(env, key_expr) {
            if let TypeExpr::Named { name: vname, .. } = self.resolve_alias_type(&vty) {
                primitive_family(vname).map(|fam| (fam, format!("type `{}`", vname), false))
            } else {
                None
            }
        } else {
            None
        };
        if let Some((got, desc, is_const_lit)) = given_info {
            if !family_compatible(want, got, is_const_lit) {
                self.diags.push(
                    Diagnostic::error(
                        "HYB-E5007",
                        self.path.clone(),
                        span,
                        format!(
                            "cannot use `{}` ({}) as `{}` value in map index (HYB-E5007)",
                            key_expr.render(),
                            desc,
                            kname
                        ),
                    )
                    .with_notes(vec![format!(
                        "the emitted Go cannot build: `cannot use {} as {} value in map index`",
                        key_expr.render(),
                        kname
                    )]),
                );
            }
        }
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
        assert_eq!(parse_int_lit("300"), Some(300));
        assert_eq!(parse_int_lit("0x12C"), Some(300));
        assert_eq!(parse_int_lit("0b101"), Some(5));
        assert_eq!(parse_int_lit("0o10"), Some(8));
    }
}
