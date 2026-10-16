//! emit_guard.rs — expression/type checks that keep accepted programs
//! emitting buildable Go (issue #201).
//!
//! Families covered here:
//!   * A — `==` / `!=` on an incomparable type (map, slice, function
//!     value, or a struct carrying one): Go rejects it with
//!     `invalid operation: … can only be compared to nil` /
//!     `cannot be compared` (HYB-E2006).
//!   * B4 — a constant negative shift count (`1 << -1`, `x <<= -1`):
//!     Go rejects it with `negative shift count` (HYB-E6005).
//!   * E — array length validity at local ascriptions (`[-1]int`,
//!     `[n]int` with a non-constant `n`): Go rejects it with
//!     `invalid array length` (HYB-E6004). Decl-level array lengths
//!     (params, struct fields, consts) are checked from `build_tables`.
//!
//! The statement/expression walk mirrors `arith.rs` (issue #149's pass):
//! flatten() lists nested bodies as their own entries, so the top-level
//! walk stays shallow (`deep = false`) and closure bodies are recursed
//! exactly once via `Expr::FnLit`.

use crate::const_overflow::{const_float_value, const_int_value};
use crate::model::{unwrap_parens, Checker, FnState, TypeShape};
use hy_diag::{Diagnostic, Span};
use hy_syntax::{
    AssignOp, BinOp, Expr, FnDecl, ForKind, InterpPart, Place, PlaceKind, SelectArmKind, Stmt,
    TypeExpr,
};

impl<'a> Checker<'a> {
    /// Pass-B entry: mirror `arith::check_stmt_const_zero_divisor`.
    pub(crate) fn check_stmt_emit_guard(&mut self, st: &FnState, stmt: &Stmt) {
        self.check_stmt_emit_guard_inner(st, stmt, /*deep=*/ false);
    }

    fn check_stmt_emit_guard_inner(&mut self, st: &FnState, stmt: &Stmt, deep: bool) {
        match stmt {
            Stmt::Local(l) => {
                if let Some(t) = &l.ty {
                    self.check_type_expr(t);
                }
                for v in &l.values {
                    self.check_expr_emit_guard(st, v);
                }
            }
            Stmt::Assign {
                targets,
                op,
                values,
                span,
            } => {
                for t in targets {
                    self.check_place_emit_guard(st, t);
                }
                for v in values {
                    self.check_expr_emit_guard(st, v);
                }
                // `x <<= -1` — the compound form of HYB-E6005.
                if matches!(op, AssignOp::ShlAssign | AssignOp::ShrAssign) && values.len() == 1 {
                    self.check_negative_shift(&values[0], *span);
                }
            }
            Stmt::Expr { expr, .. } => self.check_expr_emit_guard(st, expr),
            Stmt::Send { chan, value, .. } => {
                self.check_expr_emit_guard(st, chan);
                self.check_expr_emit_guard(st, value);
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
                        self.check_stmt_emit_guard_inner(st, init, true);
                    }
                }
                self.check_expr_emit_guard(st, cond);
                if deep {
                    for s in &then.stmts {
                        self.check_stmt_emit_guard_inner(st, s, true);
                    }
                    if let Some(other) = otherwise {
                        self.check_stmt_emit_guard_inner(st, other, true);
                    }
                }
            }
            Stmt::For { kind, body, .. } => {
                match kind {
                    ForKind::While { cond } => self.check_expr_emit_guard(st, cond),
                    ForKind::Range { lo, hi, .. } => {
                        self.check_expr_emit_guard(st, lo);
                        self.check_expr_emit_guard(st, hi);
                    }
                    ForKind::Iterate { expr, .. } => self.check_expr_emit_guard(st, expr),
                    ForKind::Infinite => {}
                }
                if deep {
                    for s in &body.stmts {
                        self.check_stmt_emit_guard_inner(st, s, true);
                    }
                }
            }
            Stmt::Return { values, .. } => {
                for v in values {
                    self.check_expr_emit_guard(st, v);
                }
            }
            Stmt::Task { callee, args, .. } => {
                self.check_expr_emit_guard(st, callee);
                for a in args {
                    self.check_expr_emit_guard(st, &a.value);
                }
            }
            Stmt::Managed { body, .. } => {
                if deep {
                    for s in &body.stmts {
                        self.check_stmt_emit_guard_inner(st, s, true);
                    }
                }
            }
            Stmt::Match { subject, arms, .. } => {
                self.check_expr_emit_guard(st, subject);
                for arm in arms {
                    if let Some(g) = &arm.guard {
                        self.check_expr_emit_guard(st, g);
                    }
                    if deep {
                        for s in &arm.body.stmts {
                            self.check_stmt_emit_guard_inner(st, s, true);
                        }
                    }
                }
            }
            Stmt::Defer { call, .. } => self.check_expr_emit_guard(st, call),
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
            Stmt::Block { block, .. } => {
                if deep {
                    for s in &block.stmts {
                        self.check_stmt_emit_guard_inner(st, s, true);
                    }
                }
            }
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    match &arm.kind {
                        SelectArmKind::Send { chan, value } => {
                            self.check_expr_emit_guard(st, chan);
                            self.check_expr_emit_guard(st, value);
                        }
                        SelectArmKind::Recv { chan, .. } => {
                            self.check_expr_emit_guard(st, chan)
                        }
                        SelectArmKind::Default => {}
                    }
                    if deep {
                        for s in &arm.body.stmts {
                            self.check_stmt_emit_guard_inner(st, s, true);
                        }
                    }
                }
            }
        }
    }

    fn check_place_emit_guard(&mut self, st: &FnState, place: &Place) {
        match &place.kind {
            PlaceKind::Ident(_) => {}
            PlaceKind::Index { base, index } => {
                self.check_place_emit_guard(st, base);
                self.check_expr_emit_guard(st, index);
            }
            PlaceKind::Field { base, .. } | PlaceKind::AddrOf { base } => {
                self.check_place_emit_guard(st, base)
            }
        }
    }

    pub(crate) fn check_expr_emit_guard(&mut self, st: &FnState, expr: &Expr) {
        match expr {
            Expr::Binary { op, lhs, rhs, span } => {
                self.check_expr_emit_guard(st, lhs);
                self.check_expr_emit_guard(st, rhs);
                match op {
                    BinOp::Eq | BinOp::Ne => self.check_equality(st, lhs, rhs, *span),
                    BinOp::Shl | BinOp::Shr => self.check_negative_shift(rhs, *span),
                    _ => {}
                }
            }
            Expr::Unary { operand, .. }
            | Expr::AddrOf { operand, .. }
            | Expr::Move { operand, .. }
            | Expr::Try { operand, .. }
            | Expr::Paren { inner: operand, .. }
            | Expr::Field { base: operand, .. }
            | Expr::Receive { chan: operand, .. } => self.check_expr_emit_guard(st, operand),
            Expr::Index { base, index, .. } => {
                self.check_expr_emit_guard(st, base);
                self.check_expr_emit_guard(st, index);
            }
            Expr::Slice { base, lo, hi, .. } => {
                self.check_expr_emit_guard(st, base);
                if let Some(lo) = lo {
                    self.check_expr_emit_guard(st, lo);
                }
                if let Some(hi) = hi {
                    self.check_expr_emit_guard(st, hi);
                }
            }
            Expr::Call { callee, args, .. } => {
                self.check_expr_emit_guard(st, callee);
                for a in args {
                    self.check_expr_emit_guard(st, &a.value);
                }
            }
            Expr::StructLit { fields, .. } => {
                for f in fields {
                    self.check_expr_emit_guard(st, &f.value);
                }
            }
            Expr::SliceLit { items, .. } => {
                for i in items {
                    self.check_expr_emit_guard(st, i);
                }
            }
            Expr::MapLit { entries, .. } => {
                for (k, v) in entries {
                    self.check_expr_emit_guard(st, k);
                    self.check_expr_emit_guard(st, v);
                }
            }
            Expr::Interp { parts, .. } => {
                for p in parts {
                    if let InterpPart::Expr(e) = p {
                        self.check_expr_emit_guard(st, e);
                    }
                }
            }
            Expr::FnLit {
                params,
                results,
                body,
                ..
            } => {
                // Closure signature types (`fn(a [-1]int) …`) are declared
                // here, not at a local ascription — check them too.
                for p in params {
                    self.check_type_expr(&p.ty);
                }
                for r in results {
                    self.check_type_expr(&r.ty);
                }
                // Closure bodies are not flattened into Pass B — deep-walk.
                for stmt in &body.stmts {
                    self.check_stmt_emit_guard_inner(st, stmt, /*deep=*/ true);
                }
            }
            Expr::Ident { .. } | Expr::Lit { .. } => {}
        }
    }

    // ------------------------------------------------------------------
    // A — equality on an incomparable type (HYB-E2006)
    // ------------------------------------------------------------------

    fn check_equality(&mut self, st: &FnState, lhs: &Expr, rhs: &Expr, span: Span) {
        // `x == nil` is the nil-comparison surface (E4001/N1 own its
        // validity) — every nilable type is comparable to nil in Go.
        if Checker::is_nil_literal(lhs) || Checker::is_nil_literal(rhs) {
            return;
        }
        let l = self.equality_shape(st, lhs);
        let r = self.equality_shape(st, rhs);
        let (bad, noun) = if let Some(n) = self.incomparable_noun(&l) {
            (lhs, n)
        } else if let Some(n) = self.incomparable_noun(&r) {
            (rhs, n)
        } else {
            return;
        };
        let msg = format!(
            "`{}` compares values of incomparable type ({}) — Go rejects this at compile time (HYB-E2006)",
            bad.render(),
            noun
        );
        let notes = vec![
            format!(
                "why: a `{}` value has no `==` in Go (`invalid operation: … {}`), so an accepted Hybrid program would emit unbuildable Go (issue #201)",
                noun,
                match noun {
                    "map" => "map can only be compared to nil",
                    "slice" => "slice can only be compared to nil",
                    "function value" => "func can only be compared to nil",
                    _ => "cannot be compared",
                }
            ),
            match noun {
                "map" => "fix: compare membership or length (`len(a) == len(b)`), or a specific entry (`a[k] == b[k]`), instead of whole-map equality".to_string(),
                "slice" => "fix: compare lengths or elements pairwise — whole-slice equality has no Go meaning".to_string(),
                "function value" => "fix: a function value is only comparable to `nil` — compare behaviour (call both) instead".to_string(),
                _ => "fix: compare the comparable fields individually — the struct carries an incomparable field".to_string(),
            },
        ];
        self.diags.push(
            Diagnostic::error("HYB-E2006", self.path.clone(), span, msg).with_notes(notes),
        );
    }

    /// Shape of an equality operand, treating a bare top-level fn name as
    /// a function VALUE (Go: `helper == helper` compares funcs —
    /// `shape_of_expr` only models fn names in call position).
    fn equality_shape(&self, st: &FnState, e: &Expr) -> TypeShape {
        if let Expr::Ident { name, .. } = unwrap_parens(e) {
            if !name.is_empty()
                && self.lookup_binding(st, name).is_none()
                && self.fns.contains_key(name.as_str())
            {
                return TypeShape::Closure;
            }
        }
        self.shape_of_expr(st, e)
    }

    /// `Some(noun)` when a `==` over this shape is provably illegal in
    /// Go. Unknown shapes stay comparable (fail-open: the checker has no
    /// type system — only PROVEN-incomparable shapes reject).
    fn incomparable_noun(&self, shape: &TypeShape) -> Option<&'static str> {
        let mut seen: Vec<String> = Vec::new();
        self.incomparable_rec(shape, &mut seen)
    }

    fn incomparable_rec(&self, shape: &TypeShape, seen: &mut Vec<String>) -> Option<&'static str> {
        match shape {
            TypeShape::Map(_, _) => Some("map"),
            TypeShape::Slice(_) => Some("slice"),
            TypeShape::Closure => Some("function value"),
            // `[N]T` is comparable exactly when `T` is.
            TypeShape::Array(elem) => self.incomparable_rec(elem, seen),
            TypeShape::Struct(Some(name)) => {
                if seen.iter().any(|n| n == name) {
                    return None; // value cycle — Go rejects the type itself elsewhere
                }
                seen.push(name.clone());
                let fields = self.structs.get(name)?;
                for (_, ft) in fields {
                    let fs = self.shape_of_type(ft);
                    if let Some(n) = self.incomparable_rec(&fs, seen) {
                        return Some(n);
                    }
                }
                None
            }
            _ => None,
        }
    }

    // ------------------------------------------------------------------
    // B4 — constant negative shift counts (HYB-E6005)
    // ------------------------------------------------------------------

    fn check_negative_shift(&mut self, rhs: &Expr, span: Span) {
        let Some(v) = const_int_value(rhs) else {
            return;
        };
        if v >= 0 {
            return;
        }
        let msg = format!(
            "shift by constant `{}` — a negative shift count is a compile error in Go (HYB-E6005)",
            rhs.render()
        );
        self.diags.push(
            Diagnostic::error("HYB-E6005", self.path.clone(), span, msg).with_notes(vec![
                format!(
                    "why: Go rejects `<< {}` with `invalid operation: negative shift count {}`, so an accepted Hybrid program would emit unbuildable Go (issue #201)",
                    v, v
                ),
                "fix: shift by a non-negative constant (shift the other operand instead: `1 << n` with `n >= 0`)".to_string(),
            ]),
        );
    }

    // ------------------------------------------------------------------
    // C — package-scope redeclarations (HYB-E1003)
    // ------------------------------------------------------------------

    /// Record a package-level name (fn / const / struct / enum / type).
    /// Go keeps ONE package block: a second declaration of the same name,
    /// in any of those kinds, is `X redeclared in this block` — the
    /// emitted module would not build (issue #201-C).
    pub(crate) fn declare_pkg_name(&mut self, name: &str, kind: &'static str, span: Span) {
        if let Some(prev) = self.pkg_names.get(name) {
            let msg = format!(
                "`{name}` redeclared in this block — first declared as a {prev}, now as a {kind} (HYB-E1003)",
                name = name,
                prev = prev,
                kind = kind
            );
            self.diags.push(
                Diagnostic::error("HYB-E1003", self.path.clone(), span, msg).with_notes(vec![
                    format!(
                        "why: Go keeps a single package block for fns, consts, structs, enums, and types — a second `{kind}` named `{name}` is `{name} redeclared in this block`, so an accepted Hybrid program would emit unbuildable Go (issue #201)",
                        kind = kind,
                        name = name
                    ),
                    format!(
                        "fix: rename one of the `{name}` declarations — methods may share a name across receiver types (they live in the receiver's method set, not package scope)",
                        name = name
                    ),
                ]),
            );
            return;
        }
        self.pkg_names.insert(name.to_string(), kind);
    }

    /// Register a method: validate the receiver (issue #201-D) and
    /// detect duplicate methods on the same receiver type (issue #201-C).
    pub(crate) fn declare_method(&mut self, f: &FnDecl) {
        let r = f.receiver.as_ref().expect("method has a receiver");
        // The receiver AS WRITTEN: the parser absorbs one `*` into
        // `is_pointer`, so `**box` shows up as `is_pointer + *box`.
        let recv_full = if r.is_pointer {
            format!("*{}", r.ty.render())
        } else {
            r.ty.render()
        };
        // -- D: receiver validity (Go: `invalid receiver type …`) --
        let recv_problem: Option<&'static str> = match &r.ty {
            // The parser absorbs one `*` into `is_pointer`; a stored
            // `*T` here means `**T`, which no Go program can declare.
            TypeExpr::Pointer { .. } => {
                Some("a pointer receiver may point at most one level (`**T` is not a Go receiver)")
            }
            TypeExpr::Named { name, .. } if is_primitive_receiver(name) => {
                Some("a method receiver must be a defined (named) type, not a primitive or interface")
            }
            TypeExpr::Slice { .. }
            | TypeExpr::Array { .. }
            | TypeExpr::Map { .. }
            | TypeExpr::Chan { .. }
            | TypeExpr::Option { .. }
            | TypeExpr::Fn { .. } => {
                Some("a method receiver must be a defined (named) type — an anonymous type cannot have methods")
            }
            _ => None,
        };
        if let Some(problem) = recv_problem {
            let msg = format!(
                "invalid receiver type `{}` for method `{}.{}` (HYB-E2007)",
                recv_full, recv_full, f.name
            );
            self.diags.push(
                Diagnostic::error("HYB-E2007", self.path.clone(), r.ty.span(), msg).with_notes(vec![
                    format!("why: {}", problem),
                    format!(
                        "why: Go rejects this with `invalid receiver type {}`, so an accepted Hybrid program would emit unbuildable Go (issue #201)",
                        recv_full
                    ),
                    format!(
                        "fix: declare the method on a struct (or named defined type): `struct {} {{ … }}` + `fn (b {}) {}(...)` — or make it a plain fn `fn {}(...)`",
                        r.ty.render(),
                        r.ty.render(),
                        f.name,
                        f.name
                    ),
                ]),
            );
        }
        // -- C: method-set duplication (`method box.get already declared`) --
        let base = r.ty.render();
        if !self.methods.insert((base.clone(), f.name.clone())) {
            let msg = format!(
                "method `{}.{}` already declared (HYB-E1003)",
                base, f.name
            );
            self.diags.push(
                Diagnostic::error("HYB-E1003", self.path.clone(), f.span, msg).with_notes(vec![
                    format!(
                        "why: Go rejects a second `func (b {}) {}` on the same receiver type (`method {}.{} already declared`), so an accepted Hybrid program would emit unbuildable Go (issue #201)",
                        base, f.name, base, f.name
                    ),
                    format!(
                        "fix: rename one of the `{}.{}` methods, or define a second receiver type for the overloaded behaviour",
                        base, f.name
                    ),
                ]),
            );
        }
    }

    // ------------------------------------------------------------------
    // E — array length validity (HYB-E6004)
    // ------------------------------------------------------------------

    /// Walk a type expression checking every array length inside it.
    pub(crate) fn check_type_expr(&mut self, ty: &TypeExpr) {
        match ty {
            TypeExpr::Array { len, elem, .. } => {
                if let Some(len) = len {
                    self.check_array_len(len);
                }
                self.check_type_expr(elem);
            }
            TypeExpr::Pointer { to, .. }
            | TypeExpr::Slice { elem: to, .. }
            | TypeExpr::Option { elem: to, .. }
            | TypeExpr::Chan { elem: to, .. } => self.check_type_expr(to),
            TypeExpr::Map { key, value, .. } => {
                self.check_type_expr(key);
                self.check_type_expr(value);
            }
            TypeExpr::Fn { params, results, .. } => {
                for p in params {
                    self.check_type_expr(&p.ty);
                }
                for r in results {
                    self.check_type_expr(r);
                }
            }
            TypeExpr::Named { .. } => {}
        }
    }

    fn check_array_len(&mut self, len: &Expr) {
        let span = len.span();
        // 1) A constant integer expression: must be non-negative.
        if let Some(v) = const_int_value(len) {
            if v < 0 {
                let msg = format!(
                    "array length `{}` is negative — Go rejects this (`invalid array length {}`), so the emitted module would not build (HYB-E6004)",
                    len.render(),
                    v
                );
                self.diags.push(
                    Diagnostic::error("HYB-E6004", self.path.clone(), span, msg).with_notes(vec![
                        "why: an array length must be a non-negative constant in Go (issue #201)".to_string(),
                        "fix: use a non-negative constant length, or declare a slice (`[]T`) if the size is dynamic".to_string(),
                    ]),
                );
            }
            return;
        }
        // 2) A non-integer constant (`[2.5]int`) — Go: truncated.
        if let Some(f) = const_float_value(len) {
            if f.fract() != 0.0 {
                let msg = format!(
                    "array length `{}` is not an integer — Go rejects this (`invalid array length`), so the emitted module would not build (HYB-E6004)",
                    len.render()
                );
                self.diags.push(
                    Diagnostic::error("HYB-E6004", self.path.clone(), span, msg).with_notes(vec![
                        "why: an array length must be an integer constant in Go (issue #201)".to_string(),
                        "fix: round the length to an integer constant, or declare a slice (`[]T`)".to_string(),
                    ]),
                );
            }
            return;
        }
        // 3) A bare identifier: must resolve to a file-level `const` —
        // a local variable (or unknown name) is not a constant length.
        if let Expr::Ident { name, .. } = unwrap_parens(len) {
            if name.is_empty() {
                return; // inferred-type sentinel, never a length
            }
            if self.consts.contains_key(name.as_str()) {
                // A file-level const: fold its value when we can prove it
                // negative (`const n = -4; var a [n]int`).
                if let Some(v) = self.const_value_of(name) {
                    if v < 0 {
                        let msg = format!(
                            "array length `{}` folds to negative constant `{}` — Go rejects this (`invalid array length {}`), so the emitted module would not build (HYB-E6004)",
                            name, v, v
                        );
                        self.diags.push(
                            Diagnostic::error("HYB-E6004", self.path.clone(), span, msg)
                                .with_notes(vec![
                                    "why: an array length must be a non-negative constant in Go (issue #201)".to_string(),
                                    "fix: declare the const with a non-negative value, or use a slice (`[]T`)".to_string(),
                                ]),
                        );
                    }
                }
                return;
            }
            let msg = format!(
                "array length `{}` is not a constant — Go requires a constant array length (`invalid array length {}`), so the emitted module would not build (HYB-E6004)",
                name, name
            );
            self.diags.push(
                Diagnostic::error("HYB-E6004", self.path.clone(), span, msg).with_notes(vec![
                    "why: Go's array length is part of the type and must be a non-negative constant; a variable length would change the type (issue #201)".to_string(),
                    "fix: use a literal or file-level `const` length, or declare a slice (`[]T`) for a dynamic size".to_string(),
                ]),
            );
            return;
        }
        // 4) Anything else (calls, `pkg.CONST`, arithmetic over idents) —
        // fail open: those can be legal constant expressions in Go.
    }

    /// Fold the file-level `const` named `name` to an integer, when its
    /// declared value is a constant expression the folder understands.
    fn const_value_of(&self, name: &str) -> Option<i128> {
        let d = self.file.decls.iter().find_map(|d| match d {
            hy_syntax::Decl::Const(c) if c.name == name => Some(c),
            _ => None,
        })?;
        const_int_value(&d.value)
    }
}

/// Go's receiver rule: the base type must be a defined type — not a
/// predeclared primitive (int/string/…) and not an interface (`error`).
/// `byte`/`rune` are aliases of primitives, so they reject too.
fn is_primitive_receiver(name: &str) -> bool {
    matches!(
        name,
        "bool"
            | "string"
            | "int"
            | "uint"
            | "uintptr"
            | "isize"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "int8"
            | "int16"
            | "int32"
            | "int64"
            | "uint8"
            | "uint16"
            | "uint32"
            | "uint64"
            | "float32"
            | "float64"
            | "byte"
            | "rune"
            | "char"
            | "error"
    )
}
