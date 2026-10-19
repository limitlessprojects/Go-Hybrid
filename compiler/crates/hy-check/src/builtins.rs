//! builtins.rs — defer-call shape + literal builtin-argument checks.
//!
//! Issue #187: programs that pass `hyb check` with 0 diagnostics but emit
//! Go that fails `go build` / `go vet`:
//!   * `defer` of a non-call (HYB-E2001 is registered but was never raised)
//!   * `make` / `len` / `cap` / `delete` / `close` / `new` with
//!     compile-time-detectable bad arity or literal misuse (HYB-E2005)
//!
//! Literal-only scope mirrors the landed #149 / #148 const-arith checks:
//! variable operands stay a run-time concern, matching Go.

use crate::model::{unwrap_parens, Checker};
use hy_diag::{Diagnostic, Span};
use hy_syntax::{Expr, ForKind, LitKind, Place, PlaceKind, SelectArmKind, Stmt, UnOp};

impl<'a> Checker<'a> {
    /// Pass-B entry: HYB-E2001 on `defer` + HYB-E2005 on builtin calls.
    /// Nested block bodies are NOT recursed — `walk::flatten` already lists
    /// those statements (same discipline as `arith.rs`).
    pub(crate) fn check_stmt_builtins(&mut self, stmt: &Stmt) {
        self.check_stmt_defer_call(stmt);
        self.check_stmt_builtin_args_inner(stmt, /*deep=*/ false);
    }

    pub(crate) fn check_stmt_defer_call(&mut self, stmt: &Stmt) {
        let Stmt::Defer { call, span } = stmt else {
            return;
        };
        let mut e = unwrap_parens(call);
        // `defer f.close()?` — Try wrapping a call is the documented form.
        if let Expr::Try { operand, .. } = e {
            e = unwrap_parens(operand);
        }
        if matches!(e, Expr::Call { .. }) {
            return;
        }
        self.diags.push(
            Diagnostic::error(
                "HYB-E2001",
                self.path.clone(),
                *span,
                "defer requires a call expression (HYB-E2001)",
            )
            .with_notes(vec![
                "Go's defer operand must be a function call; Hybrid matches that contract so accepted programs emit buildable Go".to_string(),
                "defer the call itself: `defer f.close()?` / `defer close(ch)` — not a bare ident, receive, or send".to_string(),
            ]),
        );
    }

    fn check_stmt_builtin_args_inner(&mut self, stmt: &Stmt, deep: bool) {
        match stmt {
            Stmt::Local(l) => {
                for v in &l.values {
                    self.check_expr_builtin_args(v);
                }
            }
            Stmt::Assign {
                targets, values, ..
            } => {
                for t in targets {
                    self.check_place_builtin_args(t);
                }
                for v in values {
                    self.check_expr_builtin_args(v);
                }
            }
            Stmt::Expr { expr, .. } => self.check_expr_builtin_args(expr),
            Stmt::Send { chan, value, .. } => {
                self.check_expr_builtin_args(chan);
                self.check_expr_builtin_args(value);
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
                        self.check_stmt_builtin_args_inner(init, true);
                    }
                }
                self.check_expr_builtin_args(cond);
                if deep {
                    for s in &then.stmts {
                        self.check_stmt_builtin_args_inner(s, true);
                    }
                    if let Some(other) = otherwise {
                        self.check_stmt_builtin_args_inner(other, true);
                    }
                }
            }
            Stmt::For { kind, body, .. } => {
                match kind {
                    ForKind::While { cond } => self.check_expr_builtin_args(cond),
                    ForKind::Range { lo, hi, .. } => {
                        self.check_expr_builtin_args(lo);
                        self.check_expr_builtin_args(hi);
                    }
                    ForKind::Iterate { expr, .. } => self.check_expr_builtin_args(expr),
                    ForKind::Infinite => {}
                }
                if deep {
                    for s in &body.stmts {
                        self.check_stmt_builtin_args_inner(s, true);
                    }
                }
            }
            Stmt::Return { values, .. } => {
                for v in values {
                    self.check_expr_builtin_args(v);
                }
            }
            Stmt::Task { callee, args, .. } => {
                self.check_expr_builtin_args(callee);
                for a in args {
                    self.check_expr_builtin_args(&a.value);
                }
            }
            Stmt::Managed { body, .. } => {
                if deep {
                    for s in &body.stmts {
                        self.check_stmt_builtin_args_inner(s, true);
                    }
                }
            }
            Stmt::Match { subject, arms, .. } => {
                self.check_expr_builtin_args(subject);
                for arm in arms {
                    if let Some(g) = &arm.guard {
                        self.check_expr_builtin_args(g);
                    }
                    if deep {
                        for s in &arm.body.stmts {
                            self.check_stmt_builtin_args_inner(s, true);
                        }
                    }
                }
            }
            Stmt::Defer { call, .. } => self.check_expr_builtin_args(call),
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
            Stmt::Block { block, .. } => {
                if deep {
                    for s in &block.stmts {
                        self.check_stmt_builtin_args_inner(s, true);
                    }
                }
            }
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    match &arm.kind {
                        SelectArmKind::Send { chan, value, .. } => {
                            self.check_expr_builtin_args(chan);
                            self.check_expr_builtin_args(value);
                        }
                        SelectArmKind::Recv { chan, .. } => self.check_expr_builtin_args(chan),
                        SelectArmKind::Default => {}
                    }
                    if deep {
                        for s in &arm.body.stmts {
                            self.check_stmt_builtin_args_inner(s, true);
                        }
                    }
                }
            }
        }
    }

    fn check_place_builtin_args(&mut self, place: &Place) {
        match &place.kind {
            PlaceKind::Index { base, index } => {
                self.check_place_builtin_args(base);
                self.check_expr_builtin_args(index);
            }
            PlaceKind::Field { base, .. } | PlaceKind::AddrOf { base } => {
                self.check_place_builtin_args(base);
            }
            PlaceKind::Ident(_) => {}
        }
    }

    pub(crate) fn check_expr_builtin_args(&mut self, expr: &Expr) {
        match expr {
            Expr::Call { callee, args, span } => {
                self.check_expr_builtin_args(callee);
                let is_type_arg_builtin = matches!(
                    unwrap_parens(callee),
                    Expr::Ident { name, .. }
                        if (name == "make" || name == "new") && !self.fns.contains_key(name.as_str())
                );
                for (i, a) in args.iter().enumerate() {
                    if is_type_arg_builtin && i == 0 {
                        continue;
                    }
                    self.check_expr_builtin_args(&a.value);
                }
                if let Expr::Ident { name, .. } = unwrap_parens(callee) {
                    self.check_builtin_call(name, args, *span);
                }
            }
            Expr::Binary { lhs, rhs, .. } => {
                self.check_expr_builtin_args(lhs);
                self.check_expr_builtin_args(rhs);
            }
            Expr::Unary { operand, .. }
            | Expr::AddrOf { operand, .. }
            | Expr::Move { operand, .. }
            | Expr::Try { operand, .. }
            | Expr::Paren { inner: operand, .. } => self.check_expr_builtin_args(operand),
            Expr::Receive { chan, .. } => self.check_expr_builtin_args(chan),
            Expr::Index { base, index, .. } => {
                self.check_expr_builtin_args(base);
                self.check_expr_builtin_args(index);
            }
            Expr::Slice { base, lo, hi, .. } => {
                self.check_expr_builtin_args(base);
                if let Some(lo) = lo {
                    self.check_expr_builtin_args(lo);
                }
                if let Some(hi) = hi {
                    self.check_expr_builtin_args(hi);
                }
            }
            Expr::Field { base, .. } => self.check_expr_builtin_args(base),
            Expr::StructLit { ty, fields, span } => {
                if let Some(t @ hy_syntax::TypeExpr::Chan { .. }) = ty {
                    let ty_str = t.render();
                    self.emit_builtin(
                        *span,
                        format!(
                            "channel type `{ty_str}` is not a value expression — construct a channel with `make({ty_str})` (HYB-E2005)"
                        ),
                    );
                }
                for f in fields {
                    self.check_expr_builtin_args(&f.value);
                }
            }
            Expr::SliceLit { items, .. } => {
                for it in items {
                    self.check_expr_builtin_args(it);
                }
            }
            Expr::MapLit { entries, .. } => {
                for (k, v) in entries {
                    self.check_expr_builtin_args(k);
                    self.check_expr_builtin_args(v);
                }
            }
            Expr::Interp { parts, .. } => {
                for p in parts {
                    if let hy_syntax::InterpPart::Expr(e) = p {
                        self.check_expr_builtin_args(e);
                    }
                }
            }
            Expr::FnLit { body, .. } => {
                // Closure bodies are not flattened into Pass B — deep-walk.
                for stmt in &body.stmts {
                    self.check_stmt_builtin_args_inner(stmt, /*deep=*/ true);
                }
            }
            Expr::Ident { .. } | Expr::Lit { .. } => {}
        }
    }

    fn check_builtin_call(&mut self, name: &str, args: &[hy_syntax::CallArg], span: Span) {
        if self.fns.contains_key(name) {
            return;
        }
        match name {
            "make" => self.check_make_args(args, span),
            "len" => self.check_len_cap_args("len", args, span, /*cap=*/ false),
            "cap" => self.check_len_cap_args("cap", args, span, /*cap=*/ true),
            "delete" => {
                if args.len() != 2 {
                    self.emit_builtin(
                        span,
                        format!(
                            "`delete` requires exactly 2 arguments (map, key), found {} (HYB-E2005)",
                            args.len()
                        ),
                    );
                }
            }
            "copy" => {
                if args.len() != 2 {
                    self.emit_builtin(
                        span,
                        format!(
                            "`copy` requires exactly 2 slice arguments (dst, src), found {} (HYB-E2005)",
                            args.len()
                        ),
                    );
                    return;
                }
                if is_value_literal(&args[0].value)
                    || is_numeric_or_bool_or_char_literal(&args[1].value)
                    || matches!(
                        unwrap_parens(&args[1].value),
                        Expr::Lit {
                            kind: LitKind::Nil,
                            ..
                        }
                    )
                {
                    self.emit_builtin(
                        span,
                        "`copy(dst, src)` requires a destination slice and a source slice or string (HYB-E2005)"
                            .to_string(),
                    );
                }
            }
            "clear" => {
                if args.len() != 1 {
                    self.emit_builtin(
                        span,
                        format!(
                            "`clear` requires exactly 1 map or slice argument, found {} (HYB-E2005)",
                            args.len()
                        ),
                    );
                    return;
                }
                if is_value_literal(&args[0].value) {
                    self.emit_builtin(
                        span,
                        "`clear` requires a map or slice value — not a scalar or string literal (HYB-E2005)"
                            .to_string(),
                    );
                }
            }
            "close" => {
                if args.len() != 1 {
                    self.emit_builtin(
                        span,
                        format!(
                            "`close` requires exactly 1 channel argument, found {} (HYB-E2005)",
                            args.len()
                        ),
                    );
                    return;
                }
                if is_disallowed_close_operand(&args[0].value) {
                    self.emit_builtin(
                        span,
                        "`close` requires a channel value — a literal can never be a channel (HYB-E2005)"
                            .to_string(),
                    );
                }
            }
            "new" => {
                if args.len() != 1 {
                    self.emit_builtin(
                        span,
                        format!(
                            "`new` requires exactly 1 type argument, found {} (HYB-E2005)",
                            args.len()
                        ),
                    );
                    return;
                }
                if is_value_literal(&args[0].value) {
                    self.emit_builtin(
                        span,
                        "`new` takes a type — not a value literal (HYB-E2005)".to_string(),
                    );
                }
            }
            "panic" => {
                if args.len() != 1 {
                    self.emit_builtin(
                        span,
                        format!(
                            "`panic` requires exactly 1 argument, found {} (HYB-E2005)",
                            args.len()
                        ),
                    );
                }
            }
            "recover" => {
                if !args.is_empty() {
                    self.emit_builtin(
                        span,
                        format!(
                            "`recover` takes no arguments, found {} (HYB-E2005)",
                            args.len()
                        ),
                    );
                }
            }
            "min" => {
                if args.is_empty() {
                    self.emit_builtin(
                        span,
                        "`min` requires at least 1 argument, found 0 (HYB-E2005)".to_string(),
                    );
                }
            }
            "max" => {
                if args.is_empty() {
                    self.emit_builtin(
                        span,
                        "`max` requires at least 1 argument, found 0 (HYB-E2005)".to_string(),
                    );
                }
            }
            "real" => {
                if args.len() != 1 {
                    self.emit_builtin(
                        span,
                        format!(
                            "`real` requires exactly 1 complex argument, found {} (HYB-E2005)",
                            args.len()
                        ),
                    );
                }
            }
            "imag" => {
                if args.len() != 1 {
                    self.emit_builtin(
                        span,
                        format!(
                            "`imag` requires exactly 1 complex argument, found {} (HYB-E2005)",
                            args.len()
                        ),
                    );
                }
            }
            "complex" => {
                if args.len() != 2 {
                    self.emit_builtin(
                        span,
                        format!(
                            "`complex` requires exactly 2 arguments (real, imag), found {} (HYB-E2005)",
                            args.len()
                        ),
                    );
                }
            }
            _ => {}
        }
    }

    fn classify_make_target(&self, e: &Expr) -> MakeTargetKind {
        match unwrap_parens(e) {
            Expr::SliceLit {
                elem: Some(_),
                items,
                ..
            } if items.is_empty() => MakeTargetKind::Slice,
            Expr::MapLit { entries, .. } if entries.is_empty() => MakeTargetKind::Map,
            Expr::StructLit {
                ty: Some(hy_syntax::TypeExpr::Chan { .. }),
                fields,
                ..
            } if fields.is_empty() => MakeTargetKind::Chan,
            Expr::Ident { name, .. } => {
                let base = Self::base_named_type_str(name);
                if let Some(ty) = self.type_aliases.get(base) {
                    match self.resolve_type_alias(ty) {
                        hy_syntax::TypeExpr::Slice { .. } => MakeTargetKind::Slice,
                        hy_syntax::TypeExpr::Map { .. } => MakeTargetKind::Map,
                        hy_syntax::TypeExpr::Chan { .. } => MakeTargetKind::Chan,
                        _ => MakeTargetKind::Invalid,
                    }
                } else {
                    MakeTargetKind::Invalid
                }
            }
            _ => MakeTargetKind::Invalid,
        }
    }

    fn check_make_args(&mut self, args: &[hy_syntax::CallArg], span: Span) {
        if args.is_empty() {
            self.emit_builtin(
                span,
                "`make` requires 1 or 2 arguments (type[, size]) — or 3 for a slice (type, len, cap) — found 0 (HYB-E2005)"
                    .to_string(),
            );
            return;
        }
        let target = self.classify_make_target(&args[0].value);
        match target {
            MakeTargetKind::Invalid => {
                self.emit_builtin(
                    span,
                    format!(
                        "`make` first argument `{}` must be a slice, map, or channel type (HYB-E2005)",
                        args[0].value.render()
                    ),
                );
                return;
            }
            MakeTargetKind::Slice => {
                if args.len() < 2 || args.len() > 3 {
                    self.emit_builtin(
                        span,
                        format!(
                            "`make` on a slice type requires 2 or 3 arguments (type, len[, cap]), found {} (HYB-E2005)",
                            args.len()
                        ),
                    );
                    return;
                }
            }
            MakeTargetKind::Map | MakeTargetKind::Chan => {
                if args.len() > 2 {
                    self.emit_builtin(
                        span,
                        format!(
                            "`make` requires 1 or 2 arguments (type[, size]) — or 3 for a slice (type, len, cap) — found {} (HYB-E2005)",
                            args.len()
                        ),
                    );
                    return;
                }
            }
        }
        let mut evaluated_sizes = Vec::new();
        for arg in args.iter().skip(1) {
            match classify_make_size(&arg.value) {
                MakeSize::Ok(v) => evaluated_sizes.push(Some(v)),
                MakeSize::NonLiteral => evaluated_sizes.push(None),
                MakeSize::Negative => {
                    self.emit_builtin(
                        span,
                        "`make` size must not be a negative constant (HYB-E2005)".to_string(),
                    );
                    evaluated_sizes.push(None);
                }
                MakeSize::BadLiteral(kind) => {
                    self.emit_builtin(
                        span,
                        format!(
                            "`make` size must be a non-negative integer constant or a variable — not a {kind} literal (HYB-E2005)"
                        ),
                    );
                    evaluated_sizes.push(None);
                }
            }
        }
        if matches!(target, MakeTargetKind::Slice) && evaluated_sizes.len() == 2 {
            if let (Some(len_val), Some(cap_val)) = (evaluated_sizes[0], evaluated_sizes[1]) {
                if len_val > cap_val {
                    self.emit_builtin(
                        span,
                        format!(
                            "`make` length ({len_val}) must not be larger than capacity ({cap_val}) (HYB-E2005)"
                        ),
                    );
                }
            }
        }
    }

    fn check_len_cap_args(
        &mut self,
        name: &str,
        args: &[hy_syntax::CallArg],
        span: Span,
        is_cap: bool,
    ) {
        if args.len() != 1 {
            self.emit_builtin(
                span,
                format!(
                    "`{name}` requires exactly 1 argument, found {} (HYB-E2005)",
                    args.len()
                ),
            );
            return;
        }
        let op = &args[0].value;
        if is_numeric_or_bool_or_char_literal(op) {
            self.emit_builtin(
                span,
                format!("`{name}` does not accept a number/bool/char literal operand (HYB-E2005)"),
            );
            return;
        }
        if is_cap && is_string_literal(op) {
            self.emit_builtin(
                span,
                "`cap` does not accept a string literal — strings have length, not capacity (HYB-E2005)"
                    .to_string(),
            );
        }
    }

    fn emit_builtin(&mut self, span: Span, msg: String) {
        self.diags.push(
            Diagnostic::error("HYB-E2005", self.path.clone(), span, msg).with_notes(vec![
                "Go rejects this builtin shape at compile time; Hybrid rejects it at check time so accepted programs emit buildable Go".to_string(),
            ]),
        );
    }
}

enum MakeTargetKind {
    Slice,
    Map,
    Chan,
    Invalid,
}

enum MakeSize {
    Ok(i128),
    NonLiteral,
    Negative,
    BadLiteral(&'static str),
}

fn classify_make_size(e: &Expr) -> MakeSize {
    let bare = unwrap_parens(e);
    match bare {
        Expr::Lit { kind, .. } => match kind {
            LitKind::Int(_) => match crate::const_overflow::const_int_value(bare) {
                Some(n) if n >= 0 => MakeSize::Ok(n),
                Some(_) => MakeSize::Negative,
                None => MakeSize::BadLiteral("integer"),
            },
            LitKind::Float(_) => MakeSize::BadLiteral("float"),
            LitKind::Str(_) | LitKind::RawStr(_) => MakeSize::BadLiteral("string"),
            LitKind::Bool(_) => MakeSize::BadLiteral("bool"),
            LitKind::Char(_) => MakeSize::BadLiteral("char"),
            LitKind::Nil => MakeSize::BadLiteral("nil"),
        },
        Expr::Unary {
            op: UnOp::Neg,
            operand,
            ..
        } if matches!(
            unwrap_parens(operand),
            Expr::Lit {
                kind: LitKind::Float(_),
                ..
            }
        ) =>
        {
            MakeSize::BadLiteral("float")
        }
        _ => match crate::const_overflow::const_int_value(bare) {
            Some(n) if n >= 0 => MakeSize::Ok(n),
            Some(_) => MakeSize::Negative,
            None => MakeSize::NonLiteral,
        },
    }
}

fn is_value_literal(e: &Expr) -> bool {
    match unwrap_parens(e) {
        Expr::Lit { .. } => true,
        Expr::Unary {
            op: UnOp::Neg,
            operand,
            ..
        } => matches!(unwrap_parens(operand), Expr::Lit { .. }),
        _ => false,
    }
}

fn is_disallowed_close_operand(e: &Expr) -> bool {
    // Literals can never be channels. Ident / call / field / index stay OK.
    is_value_literal(e)
}

fn is_numeric_or_bool_or_char_literal(e: &Expr) -> bool {
    match unwrap_parens(e) {
        Expr::Lit { kind, .. } => matches!(
            kind,
            LitKind::Int(_) | LitKind::Float(_) | LitKind::Bool(_) | LitKind::Char(_)
        ),
        Expr::Unary {
            op: UnOp::Neg,
            operand,
            ..
        } => matches!(
            unwrap_parens(operand),
            Expr::Lit {
                kind: LitKind::Int(_) | LitKind::Float(_),
                ..
            }
        ),
        _ => false,
    }
}

fn is_string_literal(e: &Expr) -> bool {
    matches!(
        unwrap_parens(e),
        Expr::Lit {
            kind: LitKind::Str(_) | LitKind::RawStr(_),
            ..
        }
    )
}
