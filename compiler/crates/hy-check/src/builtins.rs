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

use crate::const_overflow::{const_int_fold_with, ConstFold};
use crate::model::{unwrap_parens, Checker, FnState};
use hy_diag::{Diagnostic, Span};
use hy_syntax::{ChanDir, Expr, ForKind, LitKind, Place, PlaceKind, SelectArmKind, Stmt, UnOp};
use std::collections::HashMap;

impl<'a> Checker<'a> {
    /// Pass-B entry: HYB-E2001 on `defer` + HYB-E2005 on builtin calls.
    /// Nested block bodies are NOT recursed — `walk::flatten` already lists
    /// those statements (same discipline as `arith.rs`).
    pub(crate) fn check_stmt_builtins(&mut self, st: &FnState, stmt: &Stmt) {
        self.check_stmt_defer_call(stmt);
        self.zd_defined.clear();
        self.zd_defined.extend(st.env.keys().cloned());
        for sc in &st.scopes {
            self.zd_defined.extend(sc.keys().cloned());
        }
        self.check_stmt_builtin_args_inner(st, stmt, /*deep=*/ false);
        self.zd_defined.clear();
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

    fn check_stmt_builtin_args_inner(&mut self, st: &FnState, stmt: &Stmt, deep: bool) {
        match stmt {
            Stmt::Local(l) => {
                for v in &l.values {
                    self.check_expr_builtin_args(st, v);
                }
            }
            Stmt::Assign {
                targets, values, ..
            } => {
                for t in targets {
                    self.check_place_builtin_args(st, t);
                }
                for v in values {
                    self.check_expr_builtin_args(st, v);
                }
            }
            Stmt::Expr { expr, .. } => self.check_expr_builtin_args(st, expr),
            Stmt::Send { chan, value, .. } => {
                self.check_expr_builtin_args(st, chan);
                self.check_expr_builtin_args(st, value);
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
                        self.check_stmt_builtin_args_inner(st, init, true);
                    }
                }
                self.check_expr_builtin_args(st, cond);
                if deep {
                    for s in &then.stmts {
                        self.check_stmt_builtin_args_inner(st, s, true);
                    }
                    if let Some(other) = otherwise {
                        self.check_stmt_builtin_args_inner(st, other, true);
                    }
                }
            }
            Stmt::For { kind, body, .. } => {
                match kind {
                    ForKind::While { cond } => self.check_expr_builtin_args(st, cond),
                    ForKind::Range { lo, hi, .. } => {
                        self.check_expr_builtin_args(st, lo);
                        self.check_expr_builtin_args(st, hi);
                    }
                    ForKind::Iterate { expr, .. } => self.check_expr_builtin_args(st, expr),
                    ForKind::Infinite => {}
                }
                if deep {
                    for s in &body.stmts {
                        self.check_stmt_builtin_args_inner(st, s, true);
                    }
                }
            }
            Stmt::Return { values, .. } => {
                for v in values {
                    self.check_expr_builtin_args(st, v);
                }
            }
            Stmt::Task { callee, args, .. } => {
                self.check_expr_builtin_args(st, callee);
                for a in args {
                    self.check_expr_builtin_args(st, &a.value);
                }
            }
            Stmt::Managed { body, .. } => {
                if deep {
                    // W-016 M3: escapes inside a walked `managed` block are the
                    // declared hatch — raise the depth so HYB-E3013 stays quiet.
                    self.managed_depth += 1;
                    for s in &body.stmts {
                        self.check_stmt_builtin_args_inner(st, s, true);
                    }
                    self.managed_depth -= 1;
                }
            }
            Stmt::Match { subject, arms, .. } => {
                self.check_expr_builtin_args(st, subject);
                for arm in arms {
                    if let Some(g) = &arm.guard {
                        self.check_expr_builtin_args(st, g);
                    }
                    if deep {
                        for s in &arm.body.stmts {
                            self.check_stmt_builtin_args_inner(st, s, true);
                        }
                    }
                }
            }
            Stmt::Defer { call, .. } => self.check_expr_builtin_args(st, call),
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
            Stmt::Block { block, .. } => {
                if deep {
                    for s in &block.stmts {
                        self.check_stmt_builtin_args_inner(st, s, true);
                    }
                }
            }
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    match &arm.kind {
                        SelectArmKind::Send { chan, value, .. } => {
                            self.check_expr_builtin_args(st, chan);
                            self.check_expr_builtin_args(st, value);
                        }
                        SelectArmKind::Recv { chan, .. } => self.check_expr_builtin_args(st, chan),
                        SelectArmKind::Default => {}
                    }
                    if deep {
                        for s in &arm.body.stmts {
                            self.check_stmt_builtin_args_inner(st, s, true);
                        }
                    }
                }
            }
        }
    }

    fn check_place_builtin_args(&mut self, st: &FnState, place: &Place) {
        match &place.kind {
            PlaceKind::Index { base, index } => {
                self.check_place_builtin_args(st, base);
                self.check_expr_builtin_args(st, index);
            }
            PlaceKind::Field { base, .. } | PlaceKind::AddrOf { base } => {
                self.check_place_builtin_args(st, base);
            }
            PlaceKind::Ident(_) => {}
        }
    }

    pub(crate) fn check_expr_builtin_args(&mut self, st: &FnState, expr: &Expr) {
        match expr {
            Expr::Call { callee, args, span } => {
                self.check_expr_builtin_args(st, callee);
                let is_type_arg_builtin = matches!(
                    unwrap_parens(callee),
                    Expr::Ident { name, .. }
                        if (name == "make" || name == "new") && !self.fns.contains_key(name.as_str())
                );
                for (i, a) in args.iter().enumerate() {
                    if is_type_arg_builtin && i == 0 {
                        continue;
                    }
                    self.check_expr_builtin_args(st, &a.value);
                }
                if let Expr::Ident { name, .. } = unwrap_parens(callee) {
                    self.check_builtin_call(name, args, *span);
                }
                // W-016 M3: a closure that writes a captured binding and is
                // passed to a foreign Go call is the A5 escape perimeter
                // (HYB-E3013, SPEC §8.5 C9).
                self.check_escape_site(st, callee, args, *span);
            }
            Expr::Binary { lhs, rhs, .. } => {
                self.check_expr_builtin_args(st, lhs);
                self.check_expr_builtin_args(st, rhs);
            }
            Expr::Unary { operand, .. }
            | Expr::AddrOf { operand, .. }
            | Expr::Move { operand, .. }
            | Expr::Try { operand, .. }
            | Expr::Paren { inner: operand, .. } => self.check_expr_builtin_args(st, operand),
            Expr::Receive { chan, .. } => self.check_expr_builtin_args(st, chan),
            Expr::Index { base, index, .. } => {
                self.check_expr_builtin_args(st, base);
                self.check_expr_builtin_args(st, index);
            }
            Expr::Slice { base, lo, hi, .. } => {
                self.check_expr_builtin_args(st, base);
                if let Some(lo) = lo {
                    self.check_expr_builtin_args(st, lo);
                }
                if let Some(hi) = hi {
                    self.check_expr_builtin_args(st, hi);
                }
            }
            Expr::Field { base, .. } => self.check_expr_builtin_args(st, base),
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
                    self.check_expr_builtin_args(st, &f.value);
                }
            }
            Expr::SliceLit { items, .. } => {
                for it in items {
                    self.check_expr_builtin_args(st, it);
                }
            }
            Expr::MapLit { entries, .. } => {
                for (k, v) in entries {
                    self.check_expr_builtin_args(st, k);
                    self.check_expr_builtin_args(st, v);
                }
            }
            Expr::Interp { parts, .. } => {
                for p in parts {
                    if let hy_syntax::InterpPart::Expr(e) = p {
                        self.check_expr_builtin_args(st, e);
                    }
                }
            }
            Expr::FnLit { params, body, .. } => {
                self.push_closure_bound(params, body);
                for stmt in &body.stmts {
                    self.check_stmt_builtin_args_inner(st, stmt, /*deep=*/ true);
                }
                self.pop_closure_bound();
            }
            Expr::Ident { .. } | Expr::Lit { .. } => {}
        }
    }

    fn check_builtin_call(&mut self, name: &str, args: &[hy_syntax::CallArg], span: Span) {
        if self.fns.contains_key(name) {
            return;
        }
        match name {
            "append" => {
                // Go's append spec: `append(s, elem...)` — the slice plus at
                // least one element is two arguments minimum (#1673).
                if args.len() < 2 {
                    self.emit_builtin(
                        span,
                        format!(
                            "`append` requires at least 2 arguments (slice, elements...), found {} (HYB-E2005)",
                            args.len()
                        ),
                    );
                }
            }
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
            match self.classify_make_size(&arg.value) {
                MakeSize::Ok(v) => evaluated_sizes.push(Some(v)),
                MakeSize::NonLiteral => evaluated_sizes.push(None),
                MakeSize::Negative => {
                    self.emit_builtin(
                        span,
                        "`make` size must not be a negative constant (HYB-E2005)".to_string(),
                    );
                    evaluated_sizes.push(None);
                }
                MakeSize::Overflow => {
                    self.emit_builtin(
                        span,
                        "`make` size constant overflows `int` (HYB-E2005)".to_string(),
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
    ///   1. the callee is `<pkg>.<fn>` (`<pkg>` imported) or a method on a
    ///      foreign opaque/handle receiver from an imported package, and
    ///   2. an argument is a `fn(...) { ... }` literal (or named/aliased
    ///      closure binding) whose body writes or mutates a captured binding.
    /// A site inside `managed(...)` is the declared escape hatch and is allowed.
    fn check_escape_site(
        &mut self,
        st: &FnState,
        callee: &Expr,
        args: &[hy_syntax::CallArg],
        span: Span,
    ) {
        if self.in_managed() || !self.is_foreign_escape_callee(st, callee) {
            return;
        }
        let Some(name) = args
            .iter()
            .find_map(|a| self.arg_closure_captured_write(st, &a.value))
        else {
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

    fn is_foreign_escape_callee(&self, st: &FnState, callee: &Expr) -> bool {
        let Expr::Field { base, .. } = unwrap_parens(callee) else {
            return false;
        };
        let base = unwrap_parens(base);
        if let Expr::Ident { name, .. } = base {
            if self.lookup_binding(st, name).is_none() && self.packages.contains(name.as_str()) {
                return true;
            }
        }
        let mut recv = self.shape_of_expr(st, base);
        while let crate::model::TypeShape::Pointer(inner) = recv {
            recv = *inner;
        }
        if let crate::model::TypeShape::ForeignOpaque(ty)
        | crate::model::TypeShape::OwnHandle(ty)
        | crate::model::TypeShape::BorrowHandle(ty) = recv
        {
            return self.packages.contains(ty.split('.').next().unwrap_or(""));
        }
        false
    }

    fn arg_closure_captured_write(&self, st: &FnState, arg: &Expr) -> Option<String> {
        match unwrap_parens(arg) {
            Expr::FnLit {
                params,
                results,
                body,
                ..
            } => self.closure_writes_captured(st, params, results, body),
            // #2724: a NAMED closure binding (`cb := fn() { ... }`) or alias
            // (`cb2 := cb`) escapes just like an inline `Expr::FnLit`. Resolve
            // the binding through `FnState.closure_defs` / `resolve_alias` and
            // run the same capture-mutation walk on its body.
            Expr::Ident { name, .. } => {
                let resolved = crate::freeze::resolve_alias(st, name);
                if let Some((params, results, body)) = st
                    .closure_defs
                    .get(name.as_str())
                    .or_else(|| st.closure_defs.get(&resolved))
                {
                    if let Some(hit) = self.closure_writes_captured(st, params, results, body) {
                        return Some(hit);
                    }
                }
                st.closure_mutates
                    .get(name.as_str())
                    .or_else(|| st.closure_mutates.get(&resolved))
                    .and_then(|roots| roots.iter().min().cloned())
            }
            _ => None,
        }
    }

    /// Is this call lexically inside a `managed(...)` block? `managed` is the
    /// declared escape hatch, so an escape site there is permitted.
    fn in_managed(&self) -> bool {
        self.managed_depth > 0
    }

    fn closure_writes_captured(
        &self,
        st: &FnState,
        params: &[hy_syntax::Param],
        results: &[hy_syntax::FnResult],
        body: &hy_syntax::Block,
    ) -> Option<String> {
        type ScopeMap = HashMap<String, Option<(String, bool)>>;
        let mut init: ScopeMap = params.iter().map(|p| (p.name.clone(), None)).collect();
        init.extend(results.iter().filter_map(|r| r.name.clone().map(|n| (n, None))));
        let mut scopes = vec![init];

        let classify = |val: Option<&Expr>, sc: &ScopeMap| -> Option<(String, bool)> {
            let e = unwrap_parens(val?);
            let (inner, is_addr) = match crate::model::as_addr_of(e) {
                Some(op) => (unwrap_parens(op), true),
                None => (e, false),
            };
            let r = inner.root_ident()?;
            if r == "_" {
                return None;
            }
            if let Some(entry) = sc.get(r) {
                let (cap, was_ptr) = entry.as_ref()?;
                return Some((cap.clone(), is_addr || *was_ptr));
            }
            let is_ptr = is_addr
                || matches!(e, Expr::Ident { .. })
                    && matches!(self.shape_of_expr(st, e), crate::model::TypeShape::Pointer(_));
            Some((r.to_string(), is_ptr))
        };

        let flat = crate::walk::flatten(body);
        for (stmt, idx) in &flat.stmts {
            if let Some(evs) = flat.events.get(idx) {
                for ev in evs {
                    match ev {
                        crate::walk::ScopeEvent::Push => {
                            scopes.push(scopes.last().cloned().unwrap_or_default());
                        }
                        crate::walk::ScopeEvent::Pop if scopes.len() > 1 => {
                            scopes.pop();
                        }
                        crate::walk::ScopeEvent::SelectRecv { names, .. } => {
                            for n in *names {
                                scopes.last_mut().unwrap().insert(n.clone(), None);
                            }
                        }
                        crate::walk::ScopeEvent::MatchBind { name, subject } => {
                            let info = classify(Some(subject), scopes.last().unwrap());
                            scopes.last_mut().unwrap().insert((*name).to_string(), info);
                        }
                        _ => {}
                    }
                }
            }
            let sc = scopes.last().unwrap();
            let mut calls = Vec::new();
            crate::walk::own_calls(stmt, &mut calls);
            for cr in calls {
                if let Some(hit) = self.call_mutates_captured(st, cr.callee, cr.args, sc) {
                    return Some(hit);
                }
            }
            match stmt {
                Stmt::Local(l) => {
                    for (i, n) in l.names.iter().enumerate() {
                        let info = classify(
                            l.values.get(i).or_else(|| l.values.first()),
                            scopes.last().unwrap(),
                        );
                        scopes.last_mut().unwrap().insert(n.clone(), info);
                    }
                }
                Stmt::Assign {
                    targets,
                    op,
                    values,
                    ..
                } => {
                    for (i, t) in targets.iter().enumerate() {
                        let Some(root) = place_root(t) else {
                            continue;
                        };
                        let Some(alias) = scopes.last().unwrap().get(root).cloned() else {
                            return Some(root.to_string());
                        };
                        match &t.kind {
                            PlaceKind::Ident(name) if matches!(op, hy_syntax::AssignOp::Assign) => {
                                let info = classify(
                                    values.get(i).or_else(|| values.first()),
                                    scopes.last().unwrap(),
                                );
                                scopes.last_mut().unwrap().insert(name.clone(), info);
                            }
                            PlaceKind::Index { .. } => {
                                if let Some((c, _)) = alias {
                                    return Some(c);
                                }
                            }
                            PlaceKind::Field { .. } | PlaceKind::AddrOf { .. } => {
                                if let Some((c, true)) = alias {
                                    return Some(c);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Stmt::For { kind, .. } => match kind {
                    ForKind::Range { var, .. } => {
                        scopes.last_mut().unwrap().insert(var.clone(), None);
                    }
                    ForKind::Iterate { vars, .. } => {
                        for v in vars {
                            scopes.last_mut().unwrap().insert(v.clone(), None);
                        }
                    }
                    ForKind::While { .. } | ForKind::Infinite => {}
                },
                _ => {}
            }
        }
        None
    }

    fn call_mutates_captured(
        &self,
        st: &FnState,
        callee: &Expr,
        args: &[hy_syntax::CallArg],
        sc: &HashMap<String, Option<(String, bool)>>,
    ) -> Option<String> {
        let ref_of = |e: &Expr| -> Option<String> {
            let r = unwrap_parens(e).root_ident()?;
            if r == "_" {
                return None;
            }
            match sc.get(r) {
                Some(Some((c, _))) => Some(c.clone()),
                Some(None) => None,
                None => Some(r.to_string()),
            }
        };
        match unwrap_parens(callee) {
            Expr::Ident { name, .. }
                if !sc.contains_key(name.as_str())
                    && !self.fns.contains_key(name.as_str())
                    && matches!(name.as_str(), "delete" | "clear" | "copy") =>
            {
                return args.first().and_then(|a| ref_of(&a.value));
            }
            Expr::Field { base, name, .. } => {
                let base = unwrap_parens(base);
                if let Expr::Ident { name: pkg, .. } = base {
                    if !sc.contains_key(pkg.as_str())
                        && self.lookup_binding(st, pkg).is_none()
                        && (self.packages.contains(pkg.as_str()) || self.known_package(pkg))
                    {
                        if matches!(
                            crate::foreign::package_mode(pkg, name),
                            crate::foreign::CallMode::Mutate | crate::foreign::CallMode::Consume
                        ) {
                            return args.first().and_then(|a| ref_of(&a.value));
                        }
                        if let Some(idx) = crate::foreign::package_mutated_arg(pkg, name) {
                            return args.get(idx).and_then(|a| ref_of(&a.value));
                        }
                        return None;
                    }
                }
                if matches!(
                    name.as_str(),
                    "insert"
                        | "remove"
                        | "pop"
                        | "set"
                        | "push_string"
                        | "array_push_string"
                        | "write"
                        | "finish"
                ) {
                    return ref_of(base);
                }
            }
            _ => {}
        }
        None
    }
}

/// The root identifier of an assignment target (`m["k"]` -> `m`, `h.f` -> `h`).
fn place_root(place: &Place) -> Option<&str> {
    let root = place.root();
    if root.is_empty() || root == "_" {
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
    Overflow,
    BadLiteral(&'static str),
}

impl<'a> Checker<'a> {
fn classify_make_size(&self, e: &Expr) -> MakeSize {
    let bare = unwrap_parens(e);
    match bare {
        Expr::Lit { kind, .. } => match kind {
            LitKind::Int(_) => match crate::const_overflow::const_int_value(bare) {
                Some(n) if n > i64::MAX as i128 => MakeSize::Overflow,
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
        _ if expr_mentions_bound(bare, &self.zd_defined, &self.closure_bound) => {
            MakeSize::NonLiteral
        }
        _ => match const_int_fold_with(bare, &self.const_values, 0, None) {
            ConstFold::Val(n) if n < 0 => MakeSize::Negative,
            ConstFold::Val(n) if n > i64::MAX as i128 => MakeSize::Overflow,
            ConstFold::Val(n) => MakeSize::Ok(n),
            ConstFold::TooBig => MakeSize::Overflow,
            ConstFold::NotConst => MakeSize::NonLiteral,
        },
    }
}
}

fn expr_mentions_bound(
    e: &Expr,
    fn_bound: &std::collections::HashSet<String>,
    closure_sets: &[std::collections::HashSet<String>],
) -> bool {
    match unwrap_parens(e) {
        Expr::Ident { name, .. } => {
            fn_bound.contains(name) || closure_sets.iter().any(|s| s.contains(name))
        }
        Expr::Unary { operand, .. } => expr_mentions_bound(operand, fn_bound, closure_sets),
        Expr::Binary { lhs, rhs, .. } => {
            expr_mentions_bound(lhs, fn_bound, closure_sets)
                || expr_mentions_bound(rhs, fn_bound, closure_sets)
        }
        _ => false,
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
