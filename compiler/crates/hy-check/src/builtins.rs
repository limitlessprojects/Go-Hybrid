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
use hy_syntax::{ChanDir, Expr, ForKind, LitKind, Place, PlaceKind, SelectArmKind, Stmt, UnOp};
use std::collections::HashSet;

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
                    // W-016 M3: escapes inside a walked `managed` block are the
                    // declared hatch — raise the depth so HYB-E3013 stays quiet.
                    self.managed_depth += 1;
                    for s in &body.stmts {
                        self.check_stmt_builtin_args_inner(s, true);
                    }
                    self.managed_depth -= 1;
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
                // W-016 M3: a closure that writes a captured binding and is
                // passed to a foreign Go call is the A5 escape perimeter
                // (HYB-E3013, SPEC §8.5 C9).
                self.check_escape_site(callee, args, *span);
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
        let unwrapped = unwrap_parens(e);
        match unwrapped {
            Expr::SliceLit {
                elem: Some(_),
                items,
                ..
            } if items.is_empty() && unwrapped.as_array_lit().is_none() => MakeTargetKind::Slice,
            Expr::MapLit { entries, .. } if entries.is_empty() => MakeTargetKind::Map,
            Expr::StructLit {
                ty: Some(hy_syntax::TypeExpr::Chan { dir, .. }),
                fields,
                ..
            } if fields.is_empty() => MakeTargetKind::Chan(*dir),
            Expr::Ident { name, .. } => {
                let base = Self::base_named_type_str(name);
                if let Some(ty) = self.type_aliases.get(base) {
                    match self.resolve_type_alias(ty) {
                        hy_syntax::TypeExpr::Slice { .. } => MakeTargetKind::Slice,
                        hy_syntax::TypeExpr::Map { .. } => MakeTargetKind::Map,
                        hy_syntax::TypeExpr::Chan { dir, .. } => MakeTargetKind::Chan(dir),
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
            MakeTargetKind::Map => {
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
            MakeTargetKind::Chan(dir) => {
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
                // Go §Making slices, maps and channels: "The channel type
                // must be bidirectional." A directional make target emits
                // `make(chan<- T)` / `make(<-chan T)`, which `go build`
                // rejects at compile time — the same
                // accepted-should-buildable contract.
                if dir != ChanDir::Both {
                    self.emit_builtin(
                        span,
                        "`make` requires a bidirectional channel type — cannot make a send-only or receive-only channel (HYB-E2005)"
                            .to_string(),
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

    /// W-016 M3 — the A5 Go-escape check-site (`HYB-E3013`).
    ///
    /// Closed-world rule (SPEC-LANGUAGE §8.5 C9): no curated Go API database is
    /// consulted. The site fires when BOTH hold:
    ///   1. the callee is `<pkg>.<fn>` and `<pkg>` is an imported package, and
    ///   2. an argument is a `fn(...) { ... }` literal whose body writes a
    ///      binding that is not declared inside that literal (a capture).
    /// A site inside `managed(...)` is the declared escape hatch and is allowed.
    fn check_escape_site(&mut self, callee: &Expr, args: &[hy_syntax::CallArg], span: Span) {
        if self.in_managed_at(span.start) || !self.is_foreign_escape_callee(callee, span.start, 0) {
            return;
        }
        let Some(name) = args.iter().find_map(|a| self.arg_captured_write(&a.value, span.start, 0)) else {
            return;
        };
        self.diags.push(
            Diagnostic::error(
                "HYB-E3013",
                self.path.clone(),
                span,
                format!(
                    "closure writes the captured binding `{name}` and is passed to a foreign Go call — the escape must be explicit (HYB-E3013)"
                ),
            )
            .with_notes(vec![
                "SPEC-LANGUAGE §8.5 C9: a closure that writes a captured mutable binding and \
                 escapes into foreign Go code must take `managed(\"...\", \"interop\")` at the call \
                 site, or the capture must be `const`/non-`may_share`"
                    .to_string(),
            ]),
        );
    }

    /// Is this call lexically inside a `managed(...)` block? `managed` is the
    /// declared escape hatch, so an escape site there is permitted.
    fn in_managed_at(&self, pos: hy_diag::Pos) -> bool {
        self.managed_depth > 0
            || self.file.decls.iter().any(|d| match d {
                hy_syntax::Decl::Fn(f) if pos_in_span(f.span, pos) => {
                    f.body.as_ref().is_some_and(|b| b.stmts.iter().any(|s| stmt_has_managed(s, pos)))
                }
                _ => false,
            })
    }

    fn is_foreign_escape_callee(&self, callee: &Expr, pos: hy_diag::Pos, depth: usize) -> bool {
        let Expr::Field { base, .. } = unwrap_parens(callee) else { return false };
        depth <= 8 && match unwrap_parens(base) {
            Expr::Ident { name, .. } => match self.resolve_local_at(name, pos) {
                LocalLookup::ForeignTyped => true,
                LocalLookup::Bound(Some(v)) => self.is_foreign_origin(v, v.span().start, depth + 1),
                LocalLookup::Bound(None) => false,
                LocalLookup::Unbound => {
                    !self.fns.contains_key(name.as_str())
                        && !self.consts.contains_key(name.as_str())
                        && self.packages.contains(name.as_str())
                }
            },
            other => self.is_foreign_origin(other, pos, depth + 1),
        }
    }

    fn is_foreign_origin(&self, expr: &Expr, pos: hy_diag::Pos, depth: usize) -> bool {
        depth <= 8 && match unwrap_parens(expr) {
            Expr::Call { callee, .. } => self.is_foreign_escape_callee(callee, pos, depth + 1),
            Expr::Try { operand, .. } | Expr::AddrOf { operand, .. } | Expr::Unary { operand, .. } => {
                self.is_foreign_origin(operand, pos, depth + 1)
            }
            Expr::StructLit { ty, .. } => ty.as_ref().is_some_and(|t| self.ty_is_foreign(t)),
            Expr::Ident { name, .. } => match self.resolve_local_at(name, pos) {
                LocalLookup::ForeignTyped => true,
                LocalLookup::Bound(Some(v)) => self.is_foreign_origin(v, v.span().start, depth + 1),
                _ => false,
            },
            _ => false,
        }
    }

    fn ty_is_foreign(&self, ty: &hy_syntax::TypeExpr) -> bool {
        match ty {
            hy_syntax::TypeExpr::Named { name, .. } => name.split_once('.').is_some_and(|(p, _)| self.packages.contains(p)),
            hy_syntax::TypeExpr::Pointer { to, .. } => self.ty_is_foreign(to),
            _ => false,
        }
    }

    fn resolve_local_at(&self, target: &str, pos: hy_diag::Pos) -> LocalLookup<'a> {
        for decl in &self.file.decls {
            let hy_syntax::Decl::Fn(f) = decl else { continue };
            if !pos_in_span(f.span, pos) { continue; }
            let mut cur = LocalLookup::Unbound;
            let sig = f.receiver.iter().map(|r| (r.name.as_str(), &r.ty))
                .chain(f.params.iter().map(|p| (p.name.as_str(), &p.ty)))
                .chain(f.results.iter().filter_map(|r| Some((r.name.as_deref()?, &r.ty))));
            for (n, ty) in sig {
                if n == target {
                    cur = if self.ty_is_foreign(ty) { LocalLookup::ForeignTyped } else { LocalLookup::Bound(None) };
                }
            }
            if let Some(body) = &f.body { self.scan_stmts_local(&body.stmts, target, pos, &mut cur); }
            return cur;
        }
        LocalLookup::Unbound
    }

    fn scan_stmts_local(&self, stmts: &'a [Stmt], target: &str, pos: hy_diag::Pos, cur: &mut LocalLookup<'a>) {
        stmts.iter().take_while(|s| !pos_before(pos, s.span().start)).for_each(|s| self.scan_one_local(s, target, pos, cur));
    }

    fn scan_one_local(&self, stmt: &'a Stmt, target: &str, pos: hy_diag::Pos, cur: &mut LocalLookup<'a>) {
        match stmt {
            Stmt::Local(l) if pos_before(l.span.start, pos) => {
                let foreign = l.ty.as_ref().is_some_and(|t| self.ty_is_foreign(t));
                for (i, n) in l.names.iter().enumerate() {
                    if n == target {
                        *cur = if foreign { LocalLookup::ForeignTyped } else { LocalLookup::Bound((l.values.len() == l.names.len()).then(|| &l.values[i])) };
                    }
                }
            }
            Stmt::Assign { targets, op: hy_syntax::AssignOp::Assign, values, span } if pos_before(span.start, pos) && targets.len() == values.len() => {
                for (t, v) in targets.iter().zip(values.iter()) {
                    if matches!(&t.kind, PlaceKind::Ident(n) if n == target) { *cur = LocalLookup::Bound(Some(v)); }
                }
            }
            Stmt::If { init, then, otherwise, span, .. } if pos_in_span(*span, pos) => {
                if let Some(i) = init { self.scan_one_local(i, target, pos, cur); }
                if pos_in_span(then.span, pos) {
                    self.scan_stmts_local(&then.stmts, target, pos, cur);
                } else if let Some(o) = otherwise.as_deref().filter(|o| pos_in_span(o.span(), pos)) {
                    self.scan_one_local(o, target, pos, cur);
                }
            }
            Stmt::For { kind, body, .. } if pos_in_span(body.span, pos) => {
                if matches!(kind, ForKind::Range { var, .. } if var == target)
                    || matches!(kind, ForKind::Iterate { vars, .. } if vars.iter().any(|v| v == target))
                {
                    *cur = LocalLookup::Bound(None);
                }
                self.scan_stmts_local(&body.stmts, target, pos, cur);
            }
            Stmt::Match { arms, span, .. } if pos_in_span(*span, pos) => {
                for a in arms.iter().filter(|a| pos_in_span(a.body.span, pos)) {
                    if matches!(&a.pattern, hy_syntax::MatchPattern::Path { binding: Some(b), .. } if b == target) { *cur = LocalLookup::Bound(None); }
                    self.scan_stmts_local(&a.body.stmts, target, pos, cur);
                }
            }
            Stmt::Select { arms, span, .. } if pos_in_span(*span, pos) => {
                for a in arms.iter().filter(|a| pos_in_span(a.body.span, pos)) {
                    if matches!(&a.kind, SelectArmKind::Recv { names, .. } if names.iter().any(|n| n == target)) { *cur = LocalLookup::Bound(None); }
                    self.scan_stmts_local(&a.body.stmts, target, pos, cur);
                }
            }
            Stmt::Managed { body, span, .. } | Stmt::Block { block: body, span } if pos_in_span(*span, pos) => {
                self.scan_stmts_local(&body.stmts, target, pos, cur);
            }
            _ => {}
        }
    }

    /// W-016 M3 — does this closure argument (literal or bound local) write a
    /// binding it captures?
    fn arg_captured_write(&self, arg: &Expr, pos: hy_diag::Pos, depth: usize) -> Option<String> {
        if depth > 8 { return None; }
        match unwrap_parens(arg) {
            Expr::FnLit { params, results, body, .. } => {
                let (mut scopes, mut hit) = (vec![fn_sig_names(params, results)], None);
                self.walk_block_writes(body, &mut scopes, &mut hit);
                hit
            }
            Expr::Ident { name, .. } => match self.resolve_local_at(name, pos) {
                LocalLookup::Bound(Some(v)) => self.arg_captured_write(v, v.span().start, depth + 1),
                _ => None,
            },
            _ => None,
        }
    }

    /// Writes include `Stmt::Assign` (the compound `x++` form included) and
    /// unshadowed mutating builtins (`delete`, `clear`, `copy` dst). Any target
    /// root not currently in `scopes` at the point of use is a capture write.
    fn walk_block_writes(&self, body: &hy_syntax::Block, scopes: &mut Vec<HashSet<String>>, hit: &mut Option<String>) {
        if hit.is_none() {
            scopes.push(HashSet::new());
            for stmt in &body.stmts {
                self.walk_stmt_writes(stmt, scopes, hit);
                if hit.is_some() { break; }
            }
            scopes.pop();
        }
    }

    fn walk_stmt_writes(&self, stmt: &Stmt, scopes: &mut Vec<HashSet<String>>, hit: &mut Option<String>) {
        if hit.is_some() { return; }
        match stmt {
            Stmt::Local(l) => {
                l.values.iter().for_each(|v| self.walk_expr_writes(v, scopes, hit));
                if let Some(top) = scopes.last_mut() {
                    top.extend(l.names.iter().filter(|n| !n.is_empty() && n.as_str() != "_").cloned());
                }
            }
            Stmt::Assign { targets, values, .. } => {
                for t in targets {
                    if let Some(root) = place_root(t).filter(|r| *r != "_" && !in_scopes(scopes, r)) {
                        *hit = Some(root.to_string());
                        return;
                    }
                    self.walk_place_writes(t, scopes, hit);
                }
                values.iter().for_each(|v| self.walk_expr_writes(v, scopes, hit));
            }
            Stmt::Expr { expr, .. } | Stmt::Defer { call: expr, .. } => self.walk_expr_writes(expr, scopes, hit),
            Stmt::Send { chan, value, .. } => {
                self.walk_expr_writes(chan, scopes, hit);
                self.walk_expr_writes(value, scopes, hit);
            }
            Stmt::Return { values, .. } => values.iter().for_each(|v| self.walk_expr_writes(v, scopes, hit)),
            Stmt::Task { callee, args, .. } => {
                self.walk_expr_writes(callee, scopes, hit);
                args.iter().for_each(|a| self.walk_expr_writes(&a.value, scopes, hit));
            }
            Stmt::If { init, cond, then, otherwise, .. } => {
                scopes.push(HashSet::new());
                if let Some(i) = init { self.walk_stmt_writes(i, scopes, hit); }
                self.walk_expr_writes(cond, scopes, hit);
                self.walk_block_writes(then, scopes, hit);
                if let Some(o) = otherwise {
                    scopes.push(HashSet::new());
                    self.walk_stmt_writes(o, scopes, hit);
                    scopes.pop();
                }
                scopes.pop();
            }
            Stmt::For { kind, body, .. } => {
                let mut ls = HashSet::new();
                match kind {
                    ForKind::Range { var, lo, hi, .. } => {
                        self.walk_expr_writes(lo, scopes, hit);
                        self.walk_expr_writes(hi, scopes, hit);
                        let _ = (var != "_").then(|| ls.insert(var.clone()));
                    }
                    ForKind::Iterate { vars, expr } => {
                        self.walk_expr_writes(expr, scopes, hit);
                        ls.extend(vars.iter().filter(|v| v.as_str() != "_").cloned());
                    }
                    ForKind::While { cond } => self.walk_expr_writes(cond, scopes, hit),
                    ForKind::Infinite => {}
                }
                scopes.push(ls);
                self.walk_block_writes(body, scopes, hit);
                scopes.pop();
            }
            Stmt::Match { subject, arms, .. } => {
                self.walk_expr_writes(subject, scopes, hit);
                for arm in arms {
                    let mut s = HashSet::new();
                    if let hy_syntax::MatchPattern::Path { binding: Some(b), .. } = &arm.pattern {
                        let _ = (b != "_").then(|| s.insert(b.clone()));
                    }
                    scopes.push(s);
                    arm.guard.iter().for_each(|g| self.walk_expr_writes(g, scopes, hit));
                    self.walk_block_writes(&arm.body, scopes, hit);
                    scopes.pop();
                }
            }
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    let mut s = HashSet::new();
                    match &arm.kind {
                        SelectArmKind::Send { chan, value, .. } => {
                            self.walk_expr_writes(chan, scopes, hit);
                            self.walk_expr_writes(value, scopes, hit);
                        }
                        SelectArmKind::Recv { names, chan, .. } => {
                            self.walk_expr_writes(chan, scopes, hit);
                            s.extend(names.iter().filter(|n| n.as_str() != "_").cloned());
                        }
                        SelectArmKind::Default => {}
                    }
                    scopes.push(s);
                    self.walk_block_writes(&arm.body, scopes, hit);
                    scopes.pop();
                }
            }
            Stmt::Managed { body, .. } | Stmt::Block { block: body, .. } => self.walk_block_writes(body, scopes, hit),
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
        }
    }

    fn walk_place_writes(&self, place: &Place, scopes: &mut Vec<HashSet<String>>, hit: &mut Option<String>) {
        match &place.kind {
            PlaceKind::Index { base, index } => {
                self.walk_place_writes(base, scopes, hit);
                self.walk_expr_writes(index, scopes, hit);
            }
            PlaceKind::Field { base, .. } | PlaceKind::AddrOf { base } => self.walk_place_writes(base, scopes, hit),
            PlaceKind::Ident(_) => {}
        }
    }

    fn walk_expr_writes(&self, expr: &Expr, scopes: &mut Vec<HashSet<String>>, hit: &mut Option<String>) {
        if hit.is_some() { return; }
        match unwrap_parens(expr) {
            Expr::Call { callee, args, .. } => {
                match unwrap_parens(callee) {
                    Expr::Ident { name, .. }
                        if matches!(name.as_str(), "delete" | "clear" | "copy")
                            && !in_scopes(scopes, name)
                            && !self.fns.contains_key(name.as_str()) =>
                    {
                        if let Some(r) = args.first().and_then(|a| mutated_arg_root(&a.value)).filter(|r| !in_scopes(scopes, r)) {
                            *hit = Some(r.to_string());
                            return;
                        }
                    }
                    Expr::FnLit { params, results, body, .. } => {
                        scopes.push(fn_sig_names(params, results));
                        self.walk_block_writes(body, scopes, hit);
                        scopes.pop();
                    }
                    other => self.walk_expr_writes(other, scopes, hit),
                }
                args.iter().for_each(|a| self.walk_expr_writes(&a.value, scopes, hit));
            }
            Expr::Binary { lhs, rhs, .. } | Expr::Index { base: lhs, index: rhs, .. } => {
                self.walk_expr_writes(lhs, scopes, hit);
                self.walk_expr_writes(rhs, scopes, hit);
            }
            Expr::Unary { operand, .. } | Expr::AddrOf { operand, .. } | Expr::Move { operand, .. }
            | Expr::Try { operand, .. } | Expr::Field { base: operand, .. } | Expr::Receive { chan: operand, .. } => {
                self.walk_expr_writes(operand, scopes, hit)
            }
            Expr::Slice { base, lo, hi, .. } => {
                self.walk_expr_writes(base, scopes, hit);
                lo.iter().chain(hi.iter()).for_each(|e| self.walk_expr_writes(e, scopes, hit));
            }
            Expr::SliceLit { items, .. } => items.iter().for_each(|e| self.walk_expr_writes(e, scopes, hit)),
            Expr::MapLit { entries, .. } => entries.iter().for_each(|(k, v)| {
                self.walk_expr_writes(k, scopes, hit);
                self.walk_expr_writes(v, scopes, hit);
            }),
            Expr::StructLit { fields, .. } => fields.iter().for_each(|f| self.walk_expr_writes(&f.value, scopes, hit)),
            Expr::Interp { parts, .. } => parts.iter().for_each(|p| {
                if let hy_syntax::InterpPart::Expr(e) = p { self.walk_expr_writes(e, scopes, hit); }
            }),
            Expr::Lit { .. } | Expr::Ident { .. } | Expr::FnLit { .. } | Expr::Paren { .. } => {}
        }
    }
}

enum LocalLookup<'a> { Unbound, ForeignTyped, Bound(Option<&'a Expr>) }

fn pos_before(a: hy_diag::Pos, b: hy_diag::Pos) -> bool { (a.line, a.col) < (b.line, b.col) }

fn pos_in_span(span: Span, pos: hy_diag::Pos) -> bool {
    (span.start.line, span.start.col) <= (pos.line, pos.col) && (pos.line, pos.col) <= (span.end.line, span.end.col)
}

fn stmt_has_managed(stmt: &Stmt, pos: hy_diag::Pos) -> bool {
    pos_in_span(stmt.span(), pos) && match stmt {
        Stmt::Managed { .. } => true,
        Stmt::Block { block, .. } | Stmt::For { body: block, .. } => block.stmts.iter().any(|s| stmt_has_managed(s, pos)),
        Stmt::If { then, otherwise, .. } => {
            (pos_in_span(then.span, pos) && then.stmts.iter().any(|s| stmt_has_managed(s, pos)))
                || otherwise.as_deref().is_some_and(|o| stmt_has_managed(o, pos))
        }
        Stmt::Match { arms, .. } => arms.iter().any(|a| pos_in_span(a.body.span, pos) && a.body.stmts.iter().any(|s| stmt_has_managed(s, pos))),
        Stmt::Select { arms, .. } => arms.iter().any(|a| pos_in_span(a.body.span, pos) && a.body.stmts.iter().any(|s| stmt_has_managed(s, pos))),
        _ => false,
    }
}

fn fn_sig_names(params: &[hy_syntax::Param], results: &[hy_syntax::FnResult]) -> HashSet<String> {
    params.iter().map(|p| p.name.as_str()).chain(results.iter().filter_map(|r| r.name.as_deref()))
        .filter(|n| !n.is_empty() && *n != "_").map(str::to_string).collect()
}

fn in_scopes(scopes: &[HashSet<String>], name: &str) -> bool { scopes.iter().any(|s| s.contains(name)) }

fn mutated_arg_root(expr: &Expr) -> Option<&str> {
    match unwrap_parens(expr) {
        Expr::Ident { name, .. } if !name.is_empty() && name != "_" => Some(name.as_str()),
        Expr::Field { base, .. } | Expr::Index { base, .. } | Expr::Slice { base, .. } => mutated_arg_root(base),
        _ => None,
    }
}

/// The root identifier of an assignment target (`m["k"]` -> `m`, `h.f` -> `h`).
fn place_root(place: &Place) -> Option<&str> {
    // `Place::root()` walks Index/Field/AddrOf down to the root binding name.
    let root = place.root();
    if root.is_empty() {
        None
    } else {
        Some(root)
    }
}

enum MakeTargetKind {
    Slice,
    Map,
    Chan(ChanDir),
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
