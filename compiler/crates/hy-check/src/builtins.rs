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

use crate::const_overflow::ConstFold;
use crate::model::{unwrap_parens, unwrap_parens_and_move, Checker, FnState, TypeShape};
use hy_diag::{Diagnostic, Span};
use hy_syntax::{ChanDir, Expr, ForKind, LitKind, Place, PlaceKind, SelectArmKind, Stmt, UnOp};
use std::collections::{HashMap, HashSet};

impl<'a> Checker<'a> {
    /// Pass-B entry: HYB-E2001 on `defer` + HYB-E2005 on builtin calls.
    /// Nested block bodies are NOT recursed — `walk::flatten` already lists
    /// those statements (same discipline as `arith.rs`).
    pub(crate) fn check_stmt_builtins(&mut self, st: &FnState, stmt: &Stmt) {
        self.check_stmt_defer_call(stmt);
        self.zd_defined.clear();
        self.zd_defined.extend(
            st.env
                .iter()
                .filter(|(_, b)| !b.immutable)
                .map(|(k, _)| k.clone()),
        );
        for sc in &st.scopes {
            self.zd_defined.extend(
                sc.iter()
                    .filter(|(_, b)| !b.immutable)
                    .map(|(k, _)| k.clone()),
            );
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
                } else if is_value_literal(&args[0].value)
                    || matches!(
                        unwrap_parens(&args[0].value),
                        Expr::SliceLit { .. } | Expr::StructLit { .. }
                    )
                {
                    self.emit_builtin(
                        span,
                        "`delete` requires a map or set first argument — not a scalar, slice, or struct literal (HYB-E2005)"
                            .to_string(),
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
                    || matches!(
                        unwrap_parens(&args[0].value),
                        Expr::MapLit { .. } | Expr::StructLit { .. }
                    )
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
                if is_value_literal(&args[0].value)
                    || matches!(unwrap_parens(&args[0].value), Expr::StructLit { .. })
                {
                    self.emit_builtin(
                        span,
                        "`clear` requires a map or slice value — not a scalar, string, or struct literal (HYB-E2005)"
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
            "min" | "max" => {
                if args.is_empty() {
                    self.emit_builtin(
                        span,
                        format!("`{name}` requires at least 1 argument, found 0 (HYB-E2005)"),
                    );
                } else if args.iter().any(|a| is_nil_literal(&a.value)) {
                    self.emit_builtin(
                        span,
                        format!("`{name}` does not accept `nil` — `nil` cannot be ordered (HYB-E2005)"),
                    );
                }
            }
            "real" | "imag" => {
                if args.len() != 1 {
                    self.emit_builtin(
                        span,
                        format!(
                            "`{name}` requires exactly 1 complex argument, found {} (HYB-E2005)",
                            args.len()
                        ),
                    );
                } else if is_non_numeric_literal(&args[0].value) {
                    self.emit_builtin(
                        span,
                        format!("`{name}` requires a numeric/complex argument — not a non-numeric literal (HYB-E2005)"),
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
                } else if args.iter().any(|a| is_non_numeric_literal(&a.value)) {
                    self.emit_builtin(
                        span,
                        "`complex` requires numeric arguments — not a non-numeric literal (HYB-E2005)".to_string(),
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
        if is_numeric_or_bool_or_char_literal(op) || is_nil_literal(op) {
            self.emit_builtin(
                span,
                format!("`{name}` does not accept a number/bool/char/nil literal operand (HYB-E2005)"),
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
        if self.in_managed_at(span) { return; }
        let site_sa = self.enclosing_fn(span).map(|f| self.state_at(f, Some(span)));
        let eff_sa;
        let sa = match &site_sa { Some(s) => s, None => { eff_sa = StateAt::new(st.clone()); &eff_sa } };
        if !self.is_foreign_escape_callee(&sa.st, callee) { return; }
        let Some(name) = args.iter().find_map(|a| self.arg_closure_captured_write(st, &a.value).or_else(|| (!self.closure_bound.is_empty()).then(|| self.arg_closure_captured_write(&sa.st, &a.value)).flatten()).or_else(|| self.expr_escape_root(sa, Some(span), &a.value, 0))) else { return; };
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
            } => self.closure_writes_captured(st, params, results, body, None).map(|(r, _)| r),
            // #2724: a NAMED closure binding (`cb := fn() { ... }`) or alias
            // (`cb2 := cb`) escapes just like an inline `Expr::FnLit`. Resolve
            // the binding through `FnState.closure_defs` / `resolve_alias` and
            // run the same capture-mutation walk on its body.
            Expr::Ident { name, .. } => {
                let resolved = crate::freeze::resolve_alias(st, name);
                if let Some((params, results, body)) = st.closure_defs.get(name.as_str()).or_else(|| st.closure_defs.get(&resolved)) {
                    if let Some((hit, _)) = self.closure_writes_captured(st, params, results, body, None) {
                        return Some(hit);
                    }
                }
                if self.enclosing_fn(arg.span()).is_none() {
                    return st.closure_mutates.get(name.as_str()).or_else(|| st.closure_mutates.get(&resolved)).and_then(|roots| roots.iter().min().cloned());
                }
                None
            }
            _ => None,
        }
    }

    /// Is this call lexically inside a `managed(...)` block? `managed` is the
    /// declared escape hatch, so an escape site there is permitted.
    fn in_managed_at(&self, span: Span) -> bool { self.managed_depth > 0 || self.enclosing_fn(span).and_then(|f| f.body.as_ref()).is_some_and(|b| { let fl = crate::walk::flatten(b); fl.stmts.iter().any(|(s, i)| span_contains(s.span(), span) && fl.in_managed(*i)) }) }
    fn closure_writes_captured(&self, st: &FnState, params: &[hy_syntax::Param], results: &[hy_syntax::FnResult], body: &hy_syntax::Block, seeded: Option<Seed>) -> Option<Orig> {
        let (mut init, dep, esc_span, seed_origins, strict, mut sa, frames) = seeded.unwrap_or_else(|| (HashMap::new(), 0, fn_span(params, results, body), Vec::new(), false, self.enclosing_fn(body.span).map_or_else(|| StateAt::new(st.clone()), |ef| self.state_at(ef, Some(body.span))), Vec::new()));
        self.push_fn_sig(&mut sa, params, results, Some(&mut init)); let (mut scopes, mut local_fns) = (vec![init], HashMap::<String, LocalFn<'_>>::new());
        let map_hit = |origs: Origs| origs.into_iter().find_map(|o| seed_origins.iter().find_map(|(k, v)| (*k == o).then(|| v.clone())).or_else(|| (!strict && !span_contains(esc_span, o.1)).then_some(o)));
        let classify = |val: Option<&Expr>, sc: &ScopeMap, sa: &StateAt| -> Option<(Orig, bool)> {
            let e = unwrap_parens_and_move(val?); let (sh, (inner, is_addr)) = (self.shape_of_expr(&sa.st, e), match crate::model::as_addr_of(e) { Some(op) => (unwrap_parens_and_move(op), true), None => (e, false) });
            if let Some(r) = inner.root_ident() {
                if r == "_" { return None; }
                if let Some(entry) = sc.get(r) { let (cap, wp) = entry.as_ref()?; return if strict && is_addr { (*wp || self.expr_traverses_shared(&sa.st, inner)).then(|| (cap.clone(), true)) } else if strict { self.may_share_or_unk(&sh).then(|| (cap.clone(), matches!(sh, TypeShape::Pointer(_)))) } else { (is_addr || self.may_share_or_unk(&sh)).then(|| (cap.clone(), is_addr || *wp || matches!(sh, TypeShape::Pointer(_)))) }; }
                return (is_addr || (!matches!(sh, TypeShape::Closure) && self.may_share_or_unk(&sh))).then(|| map_hit(sa.lookup_orig(self, r)).map(|o| (o, is_addr || matches!(sh, TypeShape::Pointer(_))))).flatten();
            }
            let mut os = Vec::new(); if is_addr { self.collect_addr_origins(sa, inner, &mut os); } else { self.collect_shared_origins(sa, e, &mut os); } Some((map_hit(os)?, is_addr || matches!(sh, TypeShape::Pointer(_))))
        };
        let flat = crate::walk::flatten(body);
        for (stmt, idx) in &flat.stmts {
            for ev in flat.events.get(idx).into_iter().flatten() { match ev {
                crate::walk::ScopeEvent::Push => { sa.push(); scopes.push(scopes.last().cloned().unwrap_or_default()); }
                crate::walk::ScopeEvent::Pop if scopes.len() > 1 => { sa.pop(); scopes.pop(); }
                crate::walk::ScopeEvent::SelectRecv { names, chan } => { self.bind_select_recv(&mut sa, names, chan); for (i, n) in names.iter().enumerate() { scopes.last_mut().unwrap().insert(n.clone(), (i == 0).then(|| map_hit(sa.lookup_orig(self, n)).map(|o| (o, matches!(self.lookup_binding(&sa.st, n).map(|b| &b.shape), Some(TypeShape::Pointer(_)))))).flatten()); } }
                crate::walk::ScopeEvent::MatchBind { name, subject } => { let info = classify(Some(subject), scopes.last().unwrap(), &sa); self.bind_match_subj(&mut sa, name, subject); scopes.last_mut().unwrap().insert((*name).to_string(), info); }
                _ => {}
            } }
            let mut calls = Vec::new(); crate::walk::own_calls(stmt, &mut calls); if let Stmt::For { kind: ForKind::Iterate { expr, .. }, .. } = stmt { crate::walk::expr_calls(expr, &mut calls); }
            for cr in calls { if let Some(hit) = self.call_mutates_captured(&sa, cr.callee, cr.args, &scopes, &local_fns, dep, esc_span, &seed_origins, strict, &frames) { return Some(hit); } }
            match stmt {
                Stmt::Local(l) => {
                    let infos: Vec<_> = l.names.iter().enumerate().map(|(i, n)| (n, classify(l.values.get(i).or_else(|| l.values.first()), scopes.last().unwrap(), &sa))).collect();
                    for (i, n) in l.names.iter().enumerate() { if let Some(Expr::FnLit { params, results, body, .. }) = l.values.get(i).or_else(|| l.values.first()).map(unwrap_parens_and_move) { local_fns.insert(n.clone(), (params, results, body, scopes.clone(), sa.clone())); } else { local_fns.remove(n); } }
                    self.bind_local_stmt(&mut sa, l); for (n, info) in infos { scopes.last_mut().unwrap().insert(n.clone(), info); }
                }
                Stmt::Assign { targets, op, values, span } => {
                    for (i, t) in targets.iter().enumerate() {
                        let Some(root) = place_root(t) else { continue };
                        let Some(alias) = scopes.last().unwrap().get(root).cloned() else { if let Some(orig) = map_hit(sa.lookup_orig(self, root)) { return Some(orig); } continue; };
                        match &t.kind {
                            PlaceKind::Ident(name) if matches!(op, hy_syntax::AssignOp::Assign) => {
                                let v = values.get(i).or_else(|| values.first()); if let Some(Expr::FnLit { params, results, body, .. }) = v.map(unwrap_parens_and_move) { local_fns.insert(name.clone(), (params, results, body, scopes.clone(), sa.clone())); } else { local_fns.remove(name); }
                                let info = classify(v, scopes.last().unwrap(), &sa); if info.is_some() || !flat.in_loop(*idx) { scopes.last_mut().unwrap().insert(name.clone(), info.clone()); }
                                if info.is_some() { for sm in scopes.iter_mut().rev().take_while(|sm| sm.contains_key(name)) { sm.insert(name.clone(), info.clone()); } }
                            }
                            PlaceKind::Index { .. } | PlaceKind::Field { .. } | PlaceKind::AddrOf { .. } if alias.as_ref().is_some_and(|(_, p)| self.place_writes_shared(&sa.st, t, *p)) => return alias.map(|(c, _)| c),
                            PlaceKind::Field { .. } if alias.is_none() => if let Some((c, _)) = classify(values.get(i).or_else(|| values.first()), scopes.last().unwrap(), &sa) { for sm in scopes.iter_mut().rev().take_while(|sm| sm.contains_key(root)) { sm.insert(root.to_string(), Some((c.clone(), false))); } },
                            _ => {}
                        }
                    }
                    self.bind_assign_origs(&mut sa, targets, *op, values, *span, flat.in_loop(*idx));
                }
                Stmt::For { kind, span, .. } => self.bind_for_kind(&mut sa, kind, *span, Some(scopes.last_mut().unwrap())),
                _ => {}
            }
        }
        None
    }
    fn call_mutates_captured<'b>(&self, sa: &StateAt, callee: &Expr, args: &[hy_syntax::CallArg], scopes: &[ScopeMap], local_fns: &HashMap<String, LocalFn<'b>>, dep: usize, esc_span: Span, seed_origins: &[(Orig, Orig)], strict: bool, frames: &[CallerFrame]) -> Option<Orig> {
        let sc = scopes.last().unwrap();
        let map_hit = |origs: Origs| origs.into_iter().find_map(|o| seed_origins.iter().find_map(|(k, v)| (*k == o).then(|| v.clone())).or_else(|| (!strict && !span_contains(esc_span, o.1)).then_some(o)));
        let ref_of = |e: &Expr| -> Option<Orig> {
            let b = unwrap_parens_and_move(e);
            if let Some(r) = b.root_ident() {
                return (r != "_").then(|| match sc.get(r) {
                    Some(Some((c, wp))) if !strict || *wp || { if let Expr::Slice { base, .. } = b { if matches!(self.shape_of_expr(&sa.st, base), TypeShape::Array(_)) { return self.expr_traverses_shared(&sa.st, base).then(|| c.clone()); } } self.may_share_or_unk(&self.shape_of_expr(&sa.st, b)) || self.expr_traverses_shared(&sa.st, b) } => Some(c.clone()),
                    Some(_) => None, None => map_hit(sa.lookup_orig(self, r)),
                }).flatten();
            }
            let mut os = Vec::new(); self.collect_shared_origins(sa, b, &mut os); map_hit(os)
        };
        match unwrap_parens(callee) {
            Expr::Ident { name, .. } if !sc.contains_key(name.as_str()) && !self.fns.contains_key(name.as_str()) && matches!(name.as_str(), "delete" | "clear" | "copy") => return args.first().and_then(|a| ref_of(&a.value)),
            Expr::Field { base, name, .. } => {
                if let Expr::Ident { name: pkg, .. } = unwrap_parens(base) { if !sc.contains_key(pkg.as_str()) && self.lookup_binding(&sa.st, pkg).is_none() && (self.packages.contains(pkg.as_str()) || self.known_package(pkg)) { return if matches!(crate::foreign::package_mode(pkg, name), crate::foreign::CallMode::Mutate | crate::foreign::CallMode::Consume) { args.first().and_then(|a| ref_of(&a.value)) } else { crate::foreign::package_mutated_arg(pkg, name).and_then(|idx| args.get(idx)).and_then(|a| ref_of(&a.value)) }; } }
                if matches!(name.as_str(), "insert" | "remove" | "pop" | "set" | "push_string" | "array_push_string" | "write" | "finish") { return ref_of(unwrap_parens(base)); }
            }
            _ => {}
        }
        if dep >= 4 { return None; }
        let esc_at = |e: &Expr| self.resolve_esc_hits(sa, Some(e.span()), e, &[], dep + 1, Some(esc_span), seed_origins, strict, frames);
        let arg_val = |a: &hy_syntax::CallArg, pty: &hy_syntax::TypeExpr| -> Option<(Orig, bool)> {
            let (psh, v) = (self.shape_of_type(pty), unwrap_parens_and_move(&a.value));
            match self.may_share_or_unk(&psh).then(|| crate::model::as_addr_of(v))? {
                Some(op) => { let op = unwrap_parens_and_move(op); let c = if let Some(r) = op.root_ident() { if r == "_" { return None; } match sc.get(r) { Some(Some((c, wp))) => (*wp || self.may_share_or_unk(&self.shape_of_expr(&sa.st, op)) || self.expr_traverses_shared(&sa.st, op)).then(|| c.clone())?, Some(None) => return None, None => map_hit(sa.lookup_orig(self, r))? } } else { map_hit(self.recv_root_origs(sa, v))? }; Some((c, true)) }
                None if matches!(psh, TypeShape::Closure) => Some((match v { Expr::FnLit { params, results, body, .. } => { let mut fsc = sc.clone(); for p in params { fsc.insert(p.name.clone(), None); } self.closure_writes_captured(&sa.st, params, results, body, Some((fsc, dep + 1, esc_span, seed_origins.to_vec(), strict, sa.clone(), frames.to_vec()))) } _ => esc_at(v) }?, false)),
                None => Some((ref_of(v)?, matches!(psh, TypeShape::Pointer(_)) || matches!(self.shape_of_expr(&sa.st, v), TypeShape::Pointer(_)))),
            }
        };
        let call_decl = |d: &hy_syntax::FnDecl, recv: Option<(&hy_syntax::Receiver, &Expr, Option<(Orig, bool)>)>| -> Option<Orig> {
            let body = d.body.as_ref()?; let (mut seed, mut fso, mut fargs) = (HashMap::new(), Vec::new(), Vec::new());
            if let Some((r, b, m)) = recv { if let Some((ref c, _)) = m { fso.push(((r.name.clone(), r.ty.span()), c.clone())); } seed.insert(r.name.clone(), m); fargs.push((r.name.clone(), b.clone())); } else if let Some(r) = &d.receiver { seed.insert(r.name.clone(), None); }
            for (i, p) in d.params.iter().enumerate() { let m = args.get(i).and_then(|a| arg_val(a, &p.ty)); if let Some((ref c, _)) = m { fso.push(((p.name.clone(), p.span), c.clone())); } seed.insert(p.name.clone(), m); if let Some(a) = args.get(i) { fargs.push((p.name.clone(), a.value.clone())); } }
            if fso.is_empty() && !d.params.iter().any(|p| self.may_share_or_unk(&self.shape_of_type(&p.ty))) && !d.receiver.as_ref().is_some_and(|r| r.is_pointer || self.may_share_or_unk(&self.shape_of_type(&r.ty))) { return None; }
            let mut nfr = frames.to_vec(); nfr.push(CallerFrame { args: fargs, sa: sa.clone(), esc_span: Some(esc_span), seed_origins: seed_origins.to_vec(), strict, is_decl: true });
            self.closure_writes_captured(&FnState::default(), &d.params, &d.results, body, Some((seed, dep + 1, fn_span(&d.params, &d.results, body), fso, true, self.state_at_decl_base(d), nfr)))
        };
        let call_fnlit = |ps: &[hy_syntax::Param], rs: &[hy_syntax::FnResult], bd: &hy_syntax::Block, dscs: &[ScopeMap], def_sa: &StateAt| {
            let (mut fsc, mut fso, mut msa, mut fargs, sb) = (dscs.last().cloned().unwrap_or_default(), seed_origins.to_vec(), def_sa.clone(), Vec::new(), def_sa.orig.len() - dscs.len()); msa.merge_decls(sa, None);
            for (k, v) in &mut fsc { if v.is_none() { if let Some((d, dsp)) = def_sa.orig.iter().enumerate().rev().find_map(|(i, m)| m.get(k).map(|(sp, _)| (i, *sp))) { if d >= sb && sa.orig.get(d).and_then(|m| m.get(k)).is_some_and(|(od, _)| *od == dsp) { if let Some(Some(t)) = scopes.get(d - sb).and_then(|m| m.get(k)) { *v = Some(t.clone()); } } } } }
            for (i, p) in ps.iter().enumerate() { let mapped = args.get(i).and_then(|a| arg_val(a, &p.ty)); let pk = (p.name.clone(), p.span); fso.retain(|(k, _)| *k != pk); if let Some((ref c, _)) = mapped { fso.push((pk, c.clone())); } fsc.insert(p.name.clone(), mapped); if let Some(a) = args.get(i) { fargs.push((p.name.clone(), a.value.clone())); } }
            let mut nfr = frames.to_vec(); nfr.push(CallerFrame { args: fargs, sa: sa.clone(), esc_span: Some(esc_span), seed_origins: seed_origins.to_vec(), strict, is_decl: false });
            self.closure_writes_captured(&def_sa.st, ps, rs, bd, Some((fsc, dep + 1, esc_span, fso, strict, msa, nfr)))
        };
        match unwrap_parens(callee) {
            Expr::Ident { name, .. } if !sc.contains_key(name.as_str()) && self.lookup_binding(&sa.st, name).is_none() => call_decl(*self.fns.get(name.as_str()).or_else(|| self.fns.get(Self::base_named_type_str(name)))?, None),
            Expr::Ident { name, .. } => local_fns.get(name.as_str()).and_then(|(ps, rs, bd, dscs, dsa)| call_fnlit(ps, rs, bd, dscs, dsa)).or_else(|| sc.get(name.as_str()).and_then(|e| e.as_ref().filter(|_| matches!(self.shape_of_expr(&sa.st, callee), TypeShape::Closure)).map(|(c, _)| c.clone()))).or_else(|| esc_at(callee)),
            Expr::Field { base, name, .. } => {
                let b = unwrap_parens_and_move(base);
                if let Some((d, ptr_r, vsh)) = self.resolve_method_decl(&sa.st, b, name) {
                    let tgt = crate::model::as_addr_of(b).unwrap_or(b);
                    let recv = d.receiver.as_ref().map(|r| (r, b, if let Some(id) = tgt.root_ident() { match sc.get(id) { Some(Some((c, wp))) if if ptr_r { *wp || matches!(self.shape_of_expr(&sa.st, b), TypeShape::Pointer(_)) || self.expr_traverses_shared(&sa.st, tgt) } else { self.may_share_or_unk(&vsh) } => Some((c.clone(), ptr_r)), None if !strict && (ptr_r || self.may_share_or_unk(&vsh)) => map_hit(sa.lookup_orig(self, id)).map(|c| (c, ptr_r)), _ => None } } else if ptr_r || self.may_share_or_unk(&vsh) { map_hit(self.recv_root_origs(sa, b)).map(|c| (c, ptr_r)) } else { None }));
                    return call_decl(d, recv);
                }
                esc_at(callee)
            }
            Expr::FnLit { params, results, body, .. } => call_fnlit(params, results, body, scopes, sa),
            _ => esc_at(callee),
        }
    }
    fn enclosing_fn(&self, sp: Span) -> Option<&'a hy_syntax::FnDecl> { self.file.fns().find(|f| f.body.as_ref().is_some_and(|b| span_contains(b.span, sp))) }
    fn may_share_or_unk(&self, sh: &TypeShape) -> bool { self.shape_may_share(sh) || matches!(sh, TypeShape::Unknown) }
    fn push_fn_sig(&self, sa: &mut StateAt, params: &[hy_syntax::Param], results: &[hy_syntax::FnResult], mut init: Option<&mut ScopeMap>) {
        sa.push(); for p in params { if let Some(m) = init.as_deref_mut() { m.entry(p.name.clone()).or_insert(None); } self.define_binding(&mut sa.st, &p.name, self.shape_of_type(&p.ty), Some(p.ty.clone()), false, false, false); sa.def_orig(&p.name, p.span, vec![(p.name.clone(), p.span)]); }
        for r in results { if let Some(n) = r.name.as_deref().filter(|&n| n != "_") { if let Some(m) = init.as_deref_mut() { m.insert(n.to_string(), None); } self.define_binding(&mut sa.st, n, self.shape_of_type(&r.ty), Some(r.ty.clone()), false, false, false); sa.def_orig(n, r.ty.span(), vec![(n.to_string(), r.ty.span())]); } }
    }
    fn bind_select_recv(&self, sa: &mut StateAt, names: &[String], chan: &Expr) {
        let (elem, mut ch_os) = (match self.shape_of_expr(&sa.st, chan) { TypeShape::Chan(e) => *e, o => o }, Vec::new()); if !matches!(elem, TypeShape::Closure) && self.may_share_or_unk(&elem) { self.collect_shared_origins(sa, chan, &mut ch_os); }
        for (i, n) in names.iter().enumerate() { self.define_binding(&mut sa.st, n, if i == 0 { elem.clone() } else { TypeShape::Primitive }, None, false, false, false); sa.def_orig(n, chan.span(), if i == 0 && !ch_os.is_empty() { ch_os.clone() } else { vec![(n.clone(), chan.span())] }); }
    }
    fn bind_match_subj(&self, sa: &mut StateAt, name: &str, subject: &Expr) {
        let ty = self.ty_of_expr(&sa.st, subject).map(|x| self.resolve_type_alias(&x)).and_then(|x| match x { hy_syntax::TypeExpr::Option { elem, .. } if !matches!(&*elem, hy_syntax::TypeExpr::Named { name, .. } if name == "any") => Some(*elem), _ => None });
        let (sh, mut os) = (match self.shape_of_expr(&sa.st, subject) { TypeShape::Unknown => ty.as_ref().map_or(TypeShape::Unknown, |t| self.shape_of_type(t)), s => s }, Vec::new()); if !matches!(sh, TypeShape::Closure) && self.may_share_or_unk(&sh) { self.collect_shared_origins(sa, subject, &mut os); }
        self.define_binding(&mut sa.st, name, sh, ty, false, false, false); sa.def_orig(name, subject.span(), if os.is_empty() { vec![(name.to_string(), subject.span())] } else { os });
    }
    fn bind_for_kind(&self, sa: &mut StateAt, kind: &ForKind, fspan: Span, mut sc: Option<&mut ScopeMap>) {
        match kind {
            ForKind::Range { var, .. } => { self.define_binding(&mut sa.st, var, TypeShape::Primitive, None, false, false, false); sa.def_orig(var, fspan, vec![(var.clone(), fspan)]); if let Some(m) = sc { m.insert(var.clone(), None); } }
            ForKind::Iterate { vars, expr } => { let mut ex_os = Vec::new(); self.collect_shared_origins(sa, expr, &mut ex_os); for (v, sh) in vars.iter().zip(self.iterate_var_shapes(&sa.st, expr, vars.len())) { let shared = !matches!(sh, TypeShape::Closure) && self.may_share_or_unk(&sh) && !ex_os.is_empty(); self.define_binding(&mut sa.st, v, sh, None, false, false, false); sa.def_orig(v, expr.span(), if shared { ex_os.clone() } else { vec![(v.clone(), expr.span())] }); if let Some(m) = sc.as_deref_mut() { m.insert(v.clone(), None); } } }
            _ => {}
        }
    }
    fn rhs_origs(&self, sa: &StateAt, rhs: Option<&Expr>, sh: &TypeShape) -> Origs { let mut os = Vec::new(); if let Some(e) = rhs.map(unwrap_parens_and_move) { if let Some(op) = crate::model::as_addr_of(e) { self.collect_addr_origins(sa, op, &mut os); } else if !matches!(sh, TypeShape::Closure) && self.may_share_or_unk(sh) { self.collect_shared_origins(sa, e, &mut os); } } os }
    fn bind_local_stmt(&self, sa: &mut StateAt, l: &hy_syntax::LocalDecl) {
        let multi = (l.names.len() > 1 && l.values.len() == 1).then(|| match unwrap_parens_and_move(&l.values[0]) { Expr::Call { callee, args, .. } => (self.call_result_shapes(&sa.st, callee, args), self.call_result_tys(&sa.st, callee)), _ => (None, None) });
        let defs: Vec<_> = l.names.iter().enumerate().map(|(i, n)| {
            let (val, ty) = (l.values.get(i), l.ty.clone().or_else(|| l.values.get(i).and_then(|e| self.ty_of_expr(&sa.st, e))).or_else(|| multi.as_ref().and_then(|(_, ts)| ts.as_ref()?.get(i).cloned())));
            let sh = l.ty.as_ref().map(|t| self.shape_of_type(t)).or_else(|| val.map(|e| self.shape_of_expr(&sa.st, e))).or_else(|| multi.as_ref().and_then(|(ss, _)| ss.as_ref()?.get(i).cloned())).or_else(|| ty.as_ref().map(|t| self.shape_of_type(t))).unwrap_or(TypeShape::Unknown);
            let mut os = self.rhs_origs(sa, val.or_else(|| multi.is_some().then(|| &l.values[0])), &sh); if os.is_empty() { os.push((n.clone(), l.span)); } (n, sh, ty, os)
        }).collect();
        for (n, sh, ty, os) in defs { self.define_binding(&mut sa.st, n, sh, ty, false, false, false); if n != "_" { sa.def_orig(n, l.span, os); } }
    }
    fn bind_assign_origs(&self, sa: &mut StateAt, targets: &[Place], op: hy_syntax::AssignOp, values: &[Expr], span: Span, in_loop: bool) {
        if !matches!(op, hy_syntax::AssignOp::Assign) { return; }
        let updates: Vec<_> = targets.iter().enumerate().filter_map(|(i, t)| {
            let (root, v) = (place_root(t)?.to_string(), values.get(i).or_else(|| (values.len() == 1 && targets.len() > 1).then(|| &values[0])));
            let mut os = self.rhs_origs(sa, v, &self.place_shape(&sa.st, t).or_else(|| v.map(|e| self.shape_of_expr(&sa.st, e))).unwrap_or(TypeShape::Unknown));
            if matches!(t.kind, PlaceKind::Ident(_)) { if os.is_empty() { os.push((root.clone(), span)); } Some((root, os, in_loop)) } else { (!os.is_empty()).then_some((root, os, true)) }
        }).collect();
        for (root, os, ext) in updates { sa.assign_orig(&root, os, ext); }
    }
    fn state_at_decl_base(&self, f: &hy_syntax::FnDecl) -> StateAt {
        let mut st = FnState::default(); st.type_params.extend(f.type_params.iter().map(|t| t.name.clone())); let mut sa = StateAt::new(st);
        if let Some(r) = &f.receiver { sa.st.type_params.extend(self.receiver_type_params(&r.ty)); let (sh, ty) = if r.is_pointer { (TypeShape::Pointer(Box::new(self.shape_of_type(&r.ty))), hy_syntax::TypeExpr::Pointer { to: Box::new(r.ty.clone()), span: r.ty.span() }) } else { (self.shape_of_type(&r.ty), r.ty.clone()) }; self.define_binding(&mut sa.st, &r.name, sh, Some(ty), false, false, false); sa.def_orig(&r.name, r.ty.span(), vec![(r.name.clone(), r.ty.span())]); }
        sa
    }
    fn state_at(&self, f: &hy_syntax::FnDecl, target: Option<Span>) -> StateAt { let mut sa = self.state_at_decl_base(f); self.push_fn_sig(&mut sa, &f.params, &f.results, None); if let (Some(b), Some(t)) = (&f.body, target) { self.bind_block_at(&mut sa, b, t); } sa }
    fn bind_block_at(&self, sa: &mut StateAt, block: &hy_syntax::Block, t: Span) {
        let flat = crate::walk::flatten(block);
        for (s, idx) in &flat.stmts {
            for ev in flat.events.get(idx).into_iter().flatten() { match ev { crate::walk::ScopeEvent::Push => sa.push(), crate::walk::ScopeEvent::Pop => sa.pop(), crate::walk::ScopeEvent::SelectRecv { names, chan } => self.bind_select_recv(sa, names, chan), crate::walk::ScopeEvent::MatchBind { name, subject } => self.bind_match_subj(sa, name, subject) } }
            let mut fls = Vec::new(); crate::walk::own_fnlits(s, &mut fls);
            if let Some(Expr::FnLit { params, results, body, .. }) = fls.into_iter().find(|fl| matches!(fl, Expr::FnLit { body, .. } if span_contains(body.span, t) && body.span != t)) { self.push_fn_sig(sa, params, results, None); self.bind_block_at(sa, body, t); return; }
            let enters = match s { Stmt::If { then, otherwise, .. } => span_contains(then.span, t) || otherwise.as_ref().is_some_and(|o| span_contains(o.span(), t)), Stmt::For { body: b, .. } | Stmt::Managed { body: b, .. } | Stmt::Block { block: b, .. } => span_contains(b.span, t), Stmt::Match { arms, .. } => arms.iter().any(|a| span_contains(a.body.span, t)), Stmt::Select { arms, .. } => arms.iter().any(|a| span_contains(a.body.span, t)), _ => false };
            if (s.span().start >= t.start || span_contains(s.span(), t)) && !enters { break; }
            match s { Stmt::Local(l) => self.bind_local_stmt(sa, l), Stmt::Assign { targets, op, values, span } => self.bind_assign_origs(sa, targets, *op, values, *span, flat.in_loop(*idx)), Stmt::For { kind, span, .. } => self.bind_for_kind(sa, kind, *span, None), _ => {} }
        }
    }
    fn collect_addr_origins(&self, sa: &StateAt, e: &Expr, out: &mut Origs) {
        let e = unwrap_parens_and_move(e); if let Some(op) = crate::model::as_addr_of(e) { return self.collect_addr_origins(sa, op, out); }
        if let Some(r) = e.root_ident() { if r != "_" { extend_origs(out, sa.lookup_orig(self, r)); } return; }
        self.collect_shared_origins(sa, e, out);
    }
    fn collect_shared_origins(&self, sa: &StateAt, e: &Expr, out: &mut Origs) {
        let e = unwrap_parens_and_move(e); if let Some(op) = crate::model::as_addr_of(e) { return self.collect_addr_origins(sa, op, out); }
        let sh = self.shape_of_expr(&sa.st, e); if matches!(sh, TypeShape::Closure) || (!matches!(e, Expr::StructLit { .. }) && !self.may_share_or_unk(&sh)) { return; }
        if let Some(r) = e.root_ident() { if r != "_" { extend_origs(out, sa.lookup_orig(self, r)); } return; }
        match e {
            Expr::StructLit { fields, .. } => for f in fields { self.collect_shared_origins(sa, &f.value, out); },
            Expr::SliceLit { items, .. } => for it in items { self.collect_shared_origins(sa, it, out); },
            Expr::MapLit { entries, .. } => for (k, v) in entries { self.collect_shared_origins(sa, k, out); self.collect_shared_origins(sa, v, out); },
            Expr::Field { base, .. } | Expr::Index { base, .. } | Expr::Slice { base, .. } | Expr::Try { operand: base, .. } => self.collect_shared_origins(sa, base, out),
            Expr::Call { callee, args, .. } => { self.collect_shared_origins(sa, callee, out); for a in args { self.collect_shared_origins(sa, &a.value, out); } }
            _ => {}
        }
    }
    fn place_writes_shared(&self, st: &FnState, p: &Place, root_ptr: bool) -> bool { match &p.kind { PlaceKind::Ident(_) => root_ptr, PlaceKind::AddrOf { base } => self.place_writes_shared(st, base, root_ptr), PlaceKind::Field { base, .. } => matches!(self.place_shape(st, base), Some(TypeShape::Pointer(_) | TypeShape::Unknown) | None) || self.place_writes_shared(st, base, root_ptr), PlaceKind::Index { base, .. } => !matches!(self.place_shape(st, base), Some(TypeShape::Array(_))) || self.place_writes_shared(st, base, root_ptr) } }
    fn expr_traverses_shared(&self, st: &FnState, expr: &Expr) -> bool { match unwrap_parens_and_move(expr) { Expr::Field { base, .. } => matches!(self.shape_of_expr(st, base), TypeShape::Pointer(_) | TypeShape::Unknown) || self.expr_traverses_shared(st, base), Expr::Index { base, .. } | Expr::Slice { base, .. } => !matches!(self.shape_of_expr(st, base), TypeShape::Array(_) | TypeShape::Str) || self.expr_traverses_shared(st, base), Expr::Try { operand, .. } => self.expr_traverses_shared(st, operand), _ => false } }
    fn resolve_method_decl(&self, st: &FnState, base: &Expr, name: &str) -> Option<(&'a hy_syntax::FnDecl, bool, TypeShape)> {
        let vsh = peel_ptr(self.shape_of_expr(st, base)); let rb = match &vsh { TypeShape::Struct(Some(s)) => Some(Self::base_named_type_str(s).to_string()), _ => self.ty_of_expr(st, base).map(|t| self.resolve_type_alias(&t)).and_then(|t| Self::base_named_type(&t)) }?;
        let d = *self.method_decls.get(&(rb, name.to_string()))?; Some((d, d.receiver.as_ref()?.is_pointer || matches!(self.resolve_type_alias(&d.receiver.as_ref()?.ty), hy_syntax::TypeExpr::Pointer { .. }), vsh))
    }
    fn recv_root_origs(&self, sa: &StateAt, mut cur: &Expr) -> Origs {
        while let Some(next) = match unwrap_parens_and_move(cur) { Expr::Try { operand: o, .. } | Expr::AddrOf { operand: o, .. } | Expr::Unary { op: UnOp::Addr, operand: o, .. } => Some(o), Expr::Field { base: b, .. } | Expr::Index { base: b, .. } | Expr::Slice { base: b, .. } => Some(b), Expr::Ident { name, .. } => return if name != "_" { sa.lookup_orig(self, name) } else { Vec::new() }, e => { cur = e; None } } { cur = next; }
        let mut os = Vec::new(); self.collect_shared_origins(sa, cur, &mut os); os
    }
    fn expr_escape_root(&self, sa: &StateAt, cspan: Option<Span>, expr: &Expr, dep: usize) -> Option<String> { self.resolve_esc_hits(sa, cspan, expr, &[], dep, None, &[], false, &[]).map(|(r, _)| r) }
    fn resolve_esc_hits(&self, sa: &StateAt, cspan: Option<Span>, e: &Expr, proj: &[Proj], dep: usize, esc_span: Option<Span>, seed_origins: &[(Orig, Orig)], strict: bool, frames: &[CallerFrame]) -> Option<Orig> {
        if dep > 24 { return None; }
        let map_o = |o: Orig| seed_origins.iter().find_map(|(k, v)| (*k == o).then(|| v.clone())).or_else(|| (!strict && esc_span.map_or(true, |es| !span_contains(es, o.1))).then_some(o));
        let mut hits = Vec::new(); self.expr_method_escape(sa, cspan, e, proj, dep, Some((seed_origins, strict, esc_span, frames)), &mut HashSet::new(), &mut hits);
        for h in hits { match h {
            EscapeHit::Root(o) => if let Some(hit) = map_o(o) { return Some(hit); },
            EscapeHit::Param(pn, rem) => {
                if rem.is_empty() { if let Some(hit) = sa.lookup_orig(self, &pn).into_iter().find_map(|o| seed_origins.iter().find_map(|(k, v)| (*k == o).then(|| v.clone()))) { return Some(hit); } }
                for i in (0..frames.len()).rev() { let fr = &frames[i]; if let Some((_, ae)) = fr.args.iter().find(|(k, _)| k == &pn) { if let Some(hit) = self.resolve_esc_hits(&fr.sa, Some(ae.span()), ae, &rem, dep + 1, fr.esc_span, &fr.seed_origins, fr.strict, &frames[..i]) { return Some(hit); } break; } if fr.is_decl { break; } }
            }
        } }
        None
    }
    fn expr_method_escape(&self, sa: &StateAt, cspan: Option<Span>, expr: &Expr, proj: &[Proj], dep: usize, octx: Option<OuterCtx>, seen: &mut EscapeSeen, out: &mut Vec<EscapeHit>) {
        if dep > 24 { return; }
        match unwrap_parens_and_move(expr) {
            Expr::AddrOf { operand: o, .. } | Expr::Unary { op: UnOp::Addr, operand: o, .. } => self.expr_method_escape(sa, cspan, o, proj, dep + 1, octx, seen, out),
            Expr::FnLit { params, results, body, .. } if proj.is_empty() => { let gsp = fn_span(params, results, body); let sd = match octx { Some((so, st_f, Some(es), frs)) if span_contains(es, gsp) => (HashMap::new(), dep + 1, es, so.to_vec(), st_f, sa.clone(), frs.to_vec()), _ => (HashMap::new(), 0, gsp, Vec::new(), false, sa.clone(), Vec::new()) }; if let Some(o) = self.closure_writes_captured(&sa.st, params, results, body, Some(sd)) { push_hit(out, EscapeHit::Root(o)); } }
            Expr::Ident { name, .. } => if let Some(cv) = self.const_values.get(name.as_str()) { self.expr_method_escape(sa, cspan, cv, proj, dep + 1, octx, seen, out); } else if let Some((sp, ef)) = cspan.and_then(|sp| self.enclosing_fn(sp).map(|f| (sp, f))) { self.flow_var_in_fn(sa, ef, sp, name, proj, dep + 1, octx, seen, out); },
            Expr::StructLit { fields, .. } => match proj.first() { None => for f in fields { self.expr_method_escape(sa, cspan, &f.value, &[], dep + 1, octx, seen, out); }, Some(Proj::Field(k)) => for f in fields.iter().filter(|f| f.name == *k) { self.expr_method_escape(sa, cspan, &f.value, &proj[1..], dep + 1, octx, seen, out); }, _ => {} },
            Expr::SliceLit { items, .. } => match proj.first() { None => for it in items { self.expr_method_escape(sa, cspan, it, &[], dep + 1, octx, seen, out); }, Some(Proj::Index(Some(k))) if k.parse::<usize>().is_ok() => if let Some(it) = k.parse::<usize>().ok().and_then(|i| items.get(i)) { self.expr_method_escape(sa, cspan, it, &proj[1..], dep + 1, octx, seen, out); }, Some(Proj::Index(_)) => for it in items { self.expr_method_escape(sa, cspan, it, &proj[1..], dep + 1, octx, seen, out); }, _ => {} },
            Expr::MapLit { entries, .. } => match proj.first() { None => for (k, v) in entries { self.expr_method_escape(sa, cspan, k, &[], dep + 1, octx, seen, out); self.expr_method_escape(sa, cspan, v, &[], dep + 1, octx, seen, out); }, Some(Proj::Index(pk)) => for (_, v) in entries.iter().filter(|(k, _)| pk.as_deref().map_or(true, |pk| const_idx(k).map_or(true, |ck| ck == pk))) { self.expr_method_escape(sa, cspan, v, &proj[1..], dep + 1, octx, seen, out); }, _ => {} },
            Expr::Field { base, name, .. } => {
                let b = unwrap_parens_and_move(base);
                if proj.is_empty() {
                    if matches!(b, Expr::Ident { name: p, .. } if self.lookup_binding(&sa.st, p).is_none() && self.known_package(p)) { return; }
                    let vsh = peel_ptr(self.shape_of_expr(&sa.st, b));
                    if (matches!(&vsh, TypeShape::Struct(Some(s)) if Self::base_named_type_str(s) == "set") && !self.structs.contains_key("set") && matches!(name.as_str(), "insert" | "remove" | "clear")) || (crate::shape::receiver_type_name(&vsh).is_some() && matches!(name.as_str(), "insert" | "remove" | "pop" | "set" | "push_string" | "array_push_string" | "write" | "finish")) { for ro in self.recv_root_origs(sa, b) { push_hit(out, EscapeHit::Root(ro)); } return; }
                    if let Some((d, ptr, vsh)) = self.resolve_method_decl(&sa.st, b, name) {
                        if let (Some(r), Some(body), true) = (&d.receiver, &d.body, ptr || self.may_share_or_unk(&vsh)) {
                            let (so, st_f, es, frs) = octx.unwrap_or((&[], false, None, &[])); let mut nfr = frs.to_vec(); nfr.push(CallerFrame { args: vec![(r.name.clone(), b.clone())], sa: sa.clone(), esc_span: es, seed_origins: so.to_vec(), strict: st_f, is_decl: true });
                            let ros = self.recv_root_origs(sa, b);
                            for ro in ros.is_empty().then_some(None).into_iter().chain(ros.into_iter().map(Some)) {
                                let (mut seed, mut fso) = (HashMap::new(), Vec::new()); if let Some(ref c) = ro { fso.push(((r.name.clone(), r.ty.span()), c.clone())); } seed.insert(r.name.clone(), ro.map(|c| (c, ptr)));
                                if let Some(hit) = self.closure_writes_captured(&FnState::default(), &d.params, &d.results, body, Some((seed, dep + 1, fn_span(&d.params, &d.results, body), fso, true, self.state_at_decl_base(d), nfr.clone()))) { push_hit(out, EscapeHit::Root(hit)); }
                            }
                        }
                        return;
                    }
                }
                self.expr_method_escape(sa, cspan, b, &prepend_proj(Proj::Field(name.clone()), proj), dep + 1, octx, seen, out);
            }
            Expr::Index { base, index, .. } => self.expr_method_escape(sa, cspan, base, &prepend_proj(Proj::Index(const_idx(index)), proj), dep + 1, octx, seen, out),
            Expr::Call { callee, args, .. } => {
                let c = unwrap_parens_and_move(callee);
                if let Expr::Ident { name, .. } = c { if self.lookup_binding(&sa.st, name).is_none() && !self.fns.contains_key(name.as_str()) {
                    if name == "some" || (args.len() == 1 && self.type_aliases.contains_key(Self::base_named_type_str(name))) { if let Some(a) = args.first() { self.expr_method_escape(sa, cspan, &a.value, proj, dep + 1, octx, seen, out); } return; }
                    if name == "append" { match proj.first() { Some(Proj::Index(_)) => { if let Some(a) = args.first() { self.expr_method_escape(sa, cspan, &a.value, &prepend_proj(Proj::Index(None), &proj[1..]), dep + 1, octx, seen, out); } for a in args.iter().skip(1) { self.expr_method_escape(sa, cspan, &a.value, &proj[1..], dep + 1, octx, seen, out); } } None => for a in args { self.expr_method_escape(sa, cspan, &a.value, &[], dep + 1, octx, seen, out); }, _ => {} } return; }
                } }
                let ((t_idx, sub_proj), mut targets) = (match proj.first() { Some(Proj::Tuple(k)) => (*k, &proj[1..]), _ => (0, proj) }, Vec::<(&hy_syntax::FnDecl, &[hy_syntax::Param], &[hy_syntax::FnResult], &hy_syntax::Block, Option<(Option<&hy_syntax::Receiver>, Option<&Expr>)>)>::new());
                match c {
                    Expr::FnLit { params, results, body, .. } => if let Some(ef) = cspan.and_then(|sp| self.enclosing_fn(sp)) { targets.push((ef, params, results, body, None)); },
                    Expr::Ident { name, .. } if self.lookup_binding(&sa.st, name).is_some() => if let Some((sp, ef)) = cspan.and_then(|sp| self.enclosing_fn(sp).map(|f| (sp, f))) { if let Some(eb) = &ef.body { let mut defs = Vec::new(); let _ = Self::collect_reaching(&eb.stmts, Some(sp), name, &[], 0, &mut defs); for (rv, _, _) in defs { if let Expr::FnLit { params, results, body, .. } = unwrap_parens_and_move(rv) { targets.push((ef, params, results, body, None)); } } } },
                    Expr::Ident { name, .. } => if let Some(&d) = self.fns.get(name.as_str()).or_else(|| self.fns.get(Self::base_named_type_str(name))) { if let Some(b) = &d.body { targets.push((d, &d.params, &d.results, b, Some((d.receiver.as_ref(), None)))); } },
                    Expr::Field { base, name, .. } => { let b = unwrap_parens_and_move(base); if let Some((d, _, _)) = self.resolve_method_decl(&sa.st, b, name) { if let Some(bd) = &d.body { targets.push((d, &d.params, &d.results, bd, Some((d.receiver.as_ref(), Some(b))))); } } }
                    _ => {}
                }
                for (ef, params, results, body, decl_recv) in targets {
                    let cfn_sp = fn_span(params, results, body);
                    for (s, _) in crate::walk::flatten(body).stmts {
                        let Stmt::Return { values, span: rspan } = s else { continue };
                        let (mut chits, mut r_sa) = (Vec::new(), self.state_at(ef, Some(*rspan))); if decl_recv.is_none() { r_sa.merge_decls(sa, Some(cfn_sp)); }
                        if !values.is_empty() {
                            let (rv, rp) = if values.len() == 1 && results.len() > 1 && matches!(proj.first(), Some(Proj::Tuple(_))) { (&values[0], proj) } else { let Some(v) = values.get(t_idx) else { continue }; (v, sub_proj) };
                            if let (None, Expr::Ident { name, .. }) = (decl_recv, unwrap_parens_and_move(rv)) { if let Some((i, _)) = params.iter().enumerate().find(|(_, p)| r_sa.lookup_orig(self, name).contains(&(p.name.clone(), p.span))) { if let Some(a) = args.get(i) { self.expr_method_escape(sa, cspan, &a.value, rp, dep + 1, octx, seen, out); } continue; } }
                            self.expr_method_escape(&r_sa, Some(*rspan), rv, rp, dep + 1, None, &mut HashSet::new(), &mut chits);
                        } else if let Some(rn) = results.get(t_idx).and_then(|r| r.name.as_deref()) { self.flow_var_in_fn(&r_sa, ef, *rspan, rn, sub_proj, dep + 1, None, &mut HashSet::new(), &mut chits); }
                        for hit in chits { match hit {
                            EscapeHit::Param(pn, rem) => if let Some((_, b)) = decl_recv.and_then(|(r, rb)| r.zip(rb).filter(|(r, _)| r.name == pn)) { self.expr_method_escape(sa, cspan, b, &rem, dep + 1, octx, seen, out); } else if let Some((i, _)) = params.iter().enumerate().find(|(_, p)| p.name == pn) { if let Some(a) = args.get(i) { self.expr_method_escape(sa, cspan, &a.value, &rem, dep + 1, octx, seen, out); } } else if decl_recv.is_none() { push_hit(out, EscapeHit::Param(pn, rem)); },
                            EscapeHit::Root((pn, psp)) => if let Some((r, b)) = decl_recv.and_then(|(r, rb)| r.zip(rb).filter(|(r, _)| r.name == pn && r.ty.span() == psp)) { if r.is_pointer || matches!(self.resolve_type_alias(&r.ty), hy_syntax::TypeExpr::Pointer { .. }) || self.may_share_or_unk(&peel_ptr(self.shape_of_expr(&sa.st, b))) { for ro in self.recv_root_origs(sa, b) { push_hit(out, EscapeHit::Root(ro)); } } } else if let Some((i, p)) = params.iter().enumerate().find(|(_, p)| p.name == pn && p.span == psp) { if self.may_share_or_unk(&self.shape_of_type(&p.ty)) { if let Some(a) = args.get(i) { for ro in self.recv_root_origs(sa, &a.value) { push_hit(out, EscapeHit::Root(ro)); } } } } else if decl_recv.is_none() && !span_contains(cfn_sp, psp) { push_hit(out, EscapeHit::Root((pn, psp))); },
                        } }
                    }
                }
            }
            _ => {}
        }
    }
    fn innermost_fnlit(b: &hy_syntax::Block, sp: Span) -> Option<Span> { for (s, _) in crate::walk::flatten(b).stmts { let mut fls = Vec::new(); crate::walk::own_fnlits(s, &mut fls); for fl in fls { if let Expr::FnLit { params, results, body, .. } = fl { let fsp = fn_span(params, results, body); if span_contains(fsp, sp) { return Self::innermost_fnlit(body, sp).or(Some(fsp)); } } } } None }
    fn flow_var_in_fn(&self, sa: &StateAt, f: &hy_syntax::FnDecl, target: Span, name: &str, proj: &[Proj], dep: usize, octx: Option<OuterCtx>, seen: &mut EscapeSeen, out: &mut Vec<EscapeHit>) {
        if !seen.insert((target.start.line, target.start.col, name.to_string(), proj.to_vec())) { return; }
        let Some(b) = &f.body else { return }; let mut defs = Vec::new(); let (killed, fl) = Self::collect_reaching(&b.stmts, Some(target), name, proj, 0, &mut defs);
        for (rv, rsp, rp) in defs { let mut d_sa = self.state_at(f, Some(rsp)); if matches!(unwrap_parens_and_move(rv), Expr::FnLit { .. }) { d_sa.merge_decls(sa, None); } else if let Some(fsp) = Self::innermost_fnlit(b, rsp) { d_sa.merge_decls(sa, Some(fsp)); } self.expr_method_escape(&d_sa, Some(rsp), rv, &rp, dep, octx, seen, out); }
        if fl == 2 || (!killed && (f.params.iter().any(|p| p.name == name) || f.receiver.as_ref().is_some_and(|r| r.name == name))) { push_hit(out, EscapeHit::Param(name.to_string(), proj.to_vec())); }
    }
    fn collect_reaching<'b>(stmts: &'b [Stmt], target: Option<Span>, var: &str, proj: &[Proj], mode: u8, out: &mut Vec<(&'b Expr, Span, Vec<Proj>)>) -> (bool, u8) {
        let active = ((mode & 1 != 0) && target.is_none()).then(|| stmts.iter().position(|s| matches!(s, Stmt::Local(l) if l.names.iter().any(|n| n == var)))).flatten().map_or(stmts, |k| &stmts[..k]);
        let Some(t) = target else { if mode >= 2 { for s in active { let _ = Self::stmt_reaching(s, None, var, proj, mode, out); } return (false, 0); } return (active.iter().rev().any(|s| Self::stmt_reaching(s, None, var, proj, mode, out).0), 0); };
        let Some(ti) = active.iter().position(|s| span_contains(s.span(), t)) else { return (active.iter().take_while(|s| s.span().start < t.start).collect::<Vec<_>>().into_iter().rev().any(|s| Self::stmt_reaching(s, None, var, proj, mode, out).0), 0); };
        let (k, fl) = Self::stmt_reaching(&active[ti], Some(t), var, proj, mode, out);
        if !k && fl == 1 { let same_blk = mode == 0 || active[..ti].iter().any(|s| matches!(s, Stmt::Local(l) if l.names.iter().any(|n| n == var))); let _ = Self::collect_reaching(&active[ti + 1..], None, var, proj, if same_blk { 2 } else { 3 }, out); }
        (k || active[..ti].iter().rev().any(|s| Self::stmt_reaching(s, None, var, proj, mode, out).0), fl)
    }
    fn stmt_reaching<'b>(stmt: &'b Stmt, inside: Option<Span>, var: &str, proj: &[Proj], mode: u8, out: &mut Vec<(&'b Expr, Span, Vec<Proj>)>) -> (bool, u8) {
        if let Stmt::If { init: Some(i), .. } = stmt {
            if inside.is_some_and(|t| span_contains(i.span(), t)) { return Self::stmt_reaching(i, inside, var, proj, mode | 1, out); }
            if inside.is_none() && matches!(i.as_ref(), Stmt::Local(l) if l.names.iter().any(|n| n == var)) { return (false, 0); }
        }
        if inside.is_none() { let mut fls = Vec::new(); crate::walk::own_fnlits(stmt, &mut fls); for fl in fls { if let Expr::FnLit { params, results, body, .. } = fl { if !params.iter().any(|p| p.name == var) && !results.iter().any(|r| r.name.as_deref() == Some(var)) { let _ = Self::collect_reaching(&body.stmts, None, var, proj, 3, out); } } } }
        let branch: Option<(Vec<(Span, &[Stmt], Vec<&str>, Option<(&Expr, bool)>)>, bool, bool)> = match stmt {
            Stmt::If { then, otherwise, .. } => Some((std::iter::once((then.span, then.stmts.as_slice(), vec![], None)).chain(otherwise.as_ref().map(|o| (o.span(), std::slice::from_ref(o.as_ref()), vec![], None))).collect(), otherwise.is_some(), false)),
            Stmt::For { kind, body, .. } => Some((vec![(body.span, &body.stmts, match kind { ForKind::Range { var: v, .. } => vec![v.as_str()], ForKind::Iterate { vars, .. } => vars.iter().map(|s| s.as_str()).collect(), _ => vec![] }, match kind { ForKind::Iterate { expr, .. } => Some((expr, true)), _ => None })], false, true)),
            Stmt::Block { block: b, .. } | Stmt::Managed { body: b, .. } => Some((vec![(b.span, &b.stmts, vec![], None)], true, false)),
            Stmt::Match { subject, arms, .. } => Some((arms.iter().map(|a| (a.body.span, a.body.stmts.as_slice(), match &a.pattern { hy_syntax::MatchPattern::Path { binding: Some(b), .. } => vec![b.as_str()], _ => vec![] }, Some((subject, false)))).collect(), arms.iter().any(|a| matches!(a.pattern, hy_syntax::MatchPattern::Wildcard)), false)),
            Stmt::Select { arms, .. } => Some((arms.iter().map(|a| (a.body.span, a.body.stmts.as_slice(), match &a.kind { SelectArmKind::Recv { names, .. } => names.iter().map(|s| s.as_str()).collect(), _ => vec![] }, None)).collect(), !arms.is_empty(), false)),
            _ => None,
        };
        if let Some((arms, exh, is_loop)) = branch {
            let Some(t) = inside else { let ks: Vec<bool> = arms.iter().map(|(_, a, vs, _)| !vs.contains(&var) && Self::collect_reaching(a, None, var, proj, mode | 1, out).0).collect(); return ((mode < 2 && exh && ks.into_iter().all(|k| k)) || matches!(stmt, Stmt::If { init: Some(i), .. } if Self::stmt_reaching(i, None, var, proj, mode | 1, out).0 && mode < 2), 0); };
            let hit = arms.iter().find(|(sp, _, _, _)| span_contains(*sp, t)).map(|(_, a, vs, ex)| { let (ck, fl) = Self::collect_reaching(a, Some(t), var, proj, 1, out); if fl >= 2 { return (true, fl); } let bnd = vs.contains(&var) && { if let Some((e, is_iter)) = ex.filter(|_| vs.last() == Some(&var)) { out.push((e, stmt.span(), if is_iter { prepend_proj(Proj::Index(None), proj) } else { proj.to_vec() })); } true }; (ck || bnd, if bnd && fl == 1 { 3 } else { fl }) });
            if is_loop && !matches!(hit, Some((true, _))) && !arms[0].2.contains(&var) { let _ = Self::collect_reaching(arms[0].1, None, var, proj, 3, out); }
            if let Some((k, fl)) = hit { return (k || matches!(stmt, Stmt::If { init: Some(i), .. } if Self::stmt_reaching(i, None, var, proj, 1, out).0), fl); }
        }
        if let Some(t) = inside {
            let mut fls = Vec::new(); crate::walk::own_fnlits(stmt, &mut fls);
            if let Some(Expr::FnLit { params, results, body, .. }) = fls.into_iter().find(|fl| span_contains(fl.span(), t)) { let (ck, cfl) = Self::collect_reaching(&body.stmts, Some(t), var, proj, 1, out); return if cfl >= 2 { (true, cfl) } else if params.iter().any(|p| p.name == var) { (true, if ck { 3 } else { 2 }) } else if results.iter().any(|r| r.name.as_deref() == Some(var)) { (true, 3) } else { (ck, 1) }; }
            return (matches!(stmt, Stmt::If { init: Some(i), .. } if Self::stmt_reaching(i, None, var, proj, 1, out).0), 0);
        }
        (match stmt {
            Stmt::Local(l) => l.names.iter().enumerate().find(|(_, n)| *n == var).is_some_and(|(i, _)| { if let Some((v, np)) = l.values.get(i).map(|v| (v, proj.to_vec())).or_else(|| (l.values.len() == 1 && l.names.len() > 1).then(|| (&l.values[0], prepend_proj(Proj::Tuple(i), proj)))) { out.push((v, l.span, np)); } true }),
            Stmt::Assign { targets, op: hy_syntax::AssignOp::Assign, values, span } => targets.iter().enumerate().filter(|(_, t)| t.root() == var).fold(false, |killed, (i, t)| {
                let (Some(steps), Some((v, tup))) = (place_steps(t), values.get(i).map(|v| (v, None)).or_else(|| (values.len() == 1 && targets.len() > 1).then(|| (&values[0], Some(Proj::Tuple(i)))))) else { return killed || place_steps(t).is_some_and(|s| s.is_empty()) };
                if steps.is_empty() { out.push((v, *span, tup.map_or_else(|| proj.to_vec(), |tp| prepend_proj(tp, proj)))); return true; }
                let common = steps.len().min(proj.len());
                if !steps[..common].iter().zip(&proj[..common]).all(|(a, b)| matches!((a, b), (Proj::Field(x), Proj::Field(y)) if x == y) || matches!((a, b), (Proj::Index(x), Proj::Index(y)) if x.is_none() || y.is_none() || x == y)) { return killed; }
                let rem = if proj.len() > steps.len() { proj[steps.len()..].to_vec() } else { Vec::new() }; out.push((v, *span, if let Some(tp) = tup { prepend_proj(tp, &rem) } else { rem }));
                killed || (steps.len() <= proj.len() && steps.iter().zip(proj).all(|(a, b)| matches!((a, b), (Proj::Field(x), Proj::Field(y)) if x == y) || matches!((a, b), (Proj::Index(Some(x)), Proj::Index(Some(y))) if x == y)))
            }),
            _ => false,
        }, 0)
    }
}
type Orig = (String, Span); type Origs = Vec<Orig>; type ScopeMap = HashMap<String, Option<(Orig, bool)>>;
#[derive(Clone)] struct CallerFrame { args: Vec<(String, Expr)>, sa: StateAt, esc_span: Option<Span>, seed_origins: Vec<(Orig, Orig)>, strict: bool, is_decl: bool }
type Seed = (ScopeMap, usize, Span, Vec<(Orig, Orig)>, bool, StateAt, Vec<CallerFrame>); type OuterCtx<'b> = (&'b [(Orig, Orig)], bool, Option<Span>, &'b [CallerFrame]);
type LocalFn<'b> = (&'b [hy_syntax::Param], &'b [hy_syntax::FnResult], &'b hy_syntax::Block, Vec<ScopeMap>, StateAt); type EscapeSeen = HashSet<(u32, u32, String, Vec<Proj>)>;
#[derive(Clone)] struct StateAt { st: FnState, orig: Vec<HashMap<String, (Span, Origs)>> }
impl StateAt {
    fn new(st: FnState) -> Self { Self { orig: vec![HashMap::new(); st.scopes.len() + 1], st } }
    fn push(&mut self) { self.st.scope_push(); self.orig.push(HashMap::new()); }
    fn pop(&mut self) { self.st.scope_pop(); if self.orig.len() > 1 { self.orig.pop(); } }
    fn def_orig(&mut self, name: &str, dsp: Span, os: Origs) { let m = self.orig.last_mut().unwrap(); let d = m.get(name).map_or(dsp, |(d, _)| *d); m.insert(name.to_string(), (d, os)); }
    fn assign_orig(&mut self, name: &str, os: Origs, in_loop: bool) { let cur = self.orig.len() - 1; if let Some(d) = self.orig.iter().rposition(|m| m.contains_key(name)) { let e = self.orig[d].get_mut(name).unwrap(); if d == cur && !in_loop { e.1 = os; } else { extend_origs(&mut e.1, os); } } }
    fn lookup_orig(&self, chk: &Checker, name: &str) -> Origs { self.orig.iter().rev().find_map(|m| m.get(name).map(|(_, o)| o.clone())).unwrap_or_else(|| chk.lookup_binding(&self.st, name).map(|_| vec![(name.to_string(), Span::point(hy_diag::Pos::new(0, 0)))]).unwrap_or_default()) }
    fn merge_decls(&mut self, other: &Self, inner_fn: Option<Span>) { for (lm, om) in self.orig.iter_mut().zip(&other.orig) { for (k, (dsp, os)) in lm { if inner_fn.map_or(true, |sp| !span_contains(sp, *dsp)) && om.get(k).is_some_and(|(od, _)| od == dsp) { extend_origs(os, om[k].1.iter().cloned()); } } } }
}
#[derive(Clone, PartialEq, Eq, Hash)] enum Proj { Field(String), Index(Option<String>), Tuple(usize) }
#[derive(Clone, PartialEq, Eq)] enum EscapeHit { Root(Orig), Param(String, Vec<Proj>) }
fn extend_origs(out: &mut Origs, os: impl IntoIterator<Item = Orig>) { for o in os { if !out.contains(&o) { out.push(o); } } }
fn push_hit(out: &mut Vec<EscapeHit>, h: EscapeHit) { if !out.contains(&h) { out.push(h); } }
fn fn_span(ps: &[hy_syntax::Param], rs: &[hy_syntax::FnResult], b: &hy_syntax::Block) -> Span { ps.iter().map(|p| p.span).chain(rs.iter().map(|r| r.ty.span())).fold(b.span, Span::to) }
fn prepend_proj(h: Proj, tail: &[Proj]) -> Vec<Proj> { let mut v = Vec::with_capacity(tail.len() + 1); v.push(h); v.extend_from_slice(tail); v }
fn place_steps(mut p: &Place) -> Option<Vec<Proj>> { let mut s = Vec::new(); loop { match &p.kind { PlaceKind::Ident(_) => { s.reverse(); return Some(s); } PlaceKind::Field { base, name } => { s.push(Proj::Field(name.clone())); p = base; } PlaceKind::Index { base, index } => { s.push(Proj::Index(const_idx(index))); p = base; } PlaceKind::AddrOf { base } => p = base } } }
fn peel_ptr(mut sh: TypeShape) -> TypeShape { while let TypeShape::Pointer(i) = sh { sh = *i; } sh }
fn span_contains(outer: Span, inner: Span) -> bool { outer.start <= inner.start && inner.end <= outer.end }
fn const_idx(e: &Expr) -> Option<String> { match unwrap_parens(e) { Expr::Lit { kind: LitKind::Int(s) | LitKind::Str(s) | LitKind::RawStr(s), .. } => Some(s.clone()), _ => None } }

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
        _ => match self.const_int_fold_expr(bare, None) {
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
        || matches!(
            unwrap_parens(e),
            Expr::SliceLit { .. } | Expr::MapLit { .. } | Expr::StructLit { .. }
        )
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

fn is_nil_literal(e: &Expr) -> bool {
    matches!(unwrap_parens(e), Expr::Lit { kind: LitKind::Nil, .. })
}

fn is_non_numeric_literal(e: &Expr) -> bool {
    matches!(
        unwrap_parens(e),
        Expr::Lit {
            kind: LitKind::Str(_) | LitKind::RawStr(_) | LitKind::Bool(_) | LitKind::Nil,
            ..
        }
    )
}
