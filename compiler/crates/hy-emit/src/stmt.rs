//! Statement emission — `Emitter` impl part 2 (see lib.rs for the context).

use hy_syntax::*;

use super::{is_package_root, to_go_name, BindingKind, EmitError, Emitter};

use super::expr::ATOM;
use super::shapes::lookup_conversion;

impl Emitter {
    pub fn emit_stmt(&mut self, s: &Stmt) -> Result<(), EmitError> {
        match s {
            Stmt::Local(l) => self.emit_local(l),
            Stmt::Assign {
                targets,
                op,
                values,
                ..
            } => self.emit_assign(targets, *op, values),
            Stmt::Expr { expr, .. } => {
                // A zero-result call with `?` IS its own statement (Batch I):
                // the hoist emits the call + propagation — no value temp, so
                // there is nothing left to emit afterwards.
                let zero_try = matches!(
                    expr,
                    Expr::Try { operand, .. } if self.call_results(operand) == Some((0, true))
                );
                // E3 expression form: hoist nested `?` first (Batch G).
                self.hoist_tries_expr(expr)?;
                if zero_try {
                    return Ok(());
                }
                let text = self.emit_expr(expr, 0)?;
                self.push(&text);
                Ok(())
            }
            Stmt::Send { chan, value, .. } => {
                // `move` markers drop here: ownership transfer is a CHECKER
                // concept; Go sends the value either way.
                if Self::expr_has_hoistable_try(value) {
                    self.hoist_order_before_try(chan)?;
                } else {
                    self.hoist_tries_expr(chan)?;
                }
                self.hoist_tries_expr(value)?;
                let ch = self.emit_expr(chan, 0)?;
                let v = self.emit_expr(value, 0)?;
                self.push(&format!("{} <- {}", ch, v));
                Ok(())
            }
            Stmt::If {
                init,
                cond,
                then,
                otherwise,
                ..
            } => self.emit_if(init.as_deref(), cond, then, otherwise.as_deref()),
            Stmt::For { kind, body, .. } => self.emit_for(kind, body),
            Stmt::Return { values, .. } => self.emit_return(values),
            Stmt::Task { callee, args, .. } => {
                // task → go (spawn sites were checked by Tier-0; emission is
                // a plain goroutine).
                let c = self.emit_expr(callee, 0)?;
                let mut parts = Vec::new();
                for a in args {
                    parts.push(self.emit_expr(&a.value, 0)?);
                }
                self.push(&format!("go {}({})", c, parts.join(", ")));
                Ok(())
            }
            Stmt::Managed {
                reason,
                category,
                tickets,
                body,
                ..
            } => {
                // The escape hatch emits a bare block. The audit marker stays
                // greppable in the generated source and now carries the
                // category + reason + tickets (row 14, discussion #4) — a
                // reviewer greps the GO output, not just the .hy input.
                let mut marker =
                    format!("{{ // managed [{}] {} (audited via hyb check --report-managed)", category, reason);
                if !tickets.is_empty() {
                    marker.push_str(&format!(" tickets: {}", tickets.join(", ")));
                }
                self.push(&marker);
                self.emit_block_body(&body.stmts)?;
                self.push("}");
                Ok(())
            }
            Stmt::Defer { call, .. } => {
                let c = self.emit_expr(call, 0)?;
                self.push(&format!("defer {}", c));
                Ok(())
            }
            Stmt::Break { label, .. } => {
                // `break loop` → the Go label of the innermost labeled
                // loop in THIS function (pr-136's per-loop labels), refused
                // when no enclosing loop exists at all (pr-101: the label
                // would have no target — the checker rejects fn bodies with
                // HYB-E3010; this guards closure bodies).
                match label {
                    Some(_) => {
                        if self.for_depth == 0 && self.loop_labels.is_empty() {
                            return Err(EmitError::unsupported(
                                "`break loop` with no enclosing loop in this function or closure \
                                 — the loop label would have no target (LX14)",
                            ));
                        }
                        self.flush_loans_for_loop(true);
                        let name = self
                            .loop_labels
                            .last()
                            .cloned()
                            .unwrap_or_else(|| "hyLoop".to_string());
                        self.push(&format!("break {}", name));
                    }
                    None => {
                        self.flush_loans_for_loop(false);
                        self.push("break");
                    }
                }
                Ok(())
            }
            Stmt::Continue { label, .. } => {
                match label {
                    Some(_) => {
                        if self.for_depth == 0 && self.loop_labels.is_empty() {
                            return Err(EmitError::unsupported(
                                "`continue loop` with no enclosing loop in this function or \
                                 closure — the loop label would have no target (LX14)",
                            ));
                        }
                        self.flush_loans_for_loop(true);
                        let name = self
                            .loop_labels
                            .last()
                            .cloned()
                            .unwrap_or_else(|| "hyLoop".to_string());
                        self.push(&format!("continue {}", name));
                    }
                    None => {
                        self.flush_loans_for_loop(false);
                        self.push("continue");
                    }
                }
                Ok(())
            }
            Stmt::Block { block, .. } => {
                self.push("{");
                self.emit_block_body(&block.stmts)?;
                self.push("}");
                Ok(())
            }
            Stmt::Match { subject, arms, .. } => self.emit_match(subject, arms),
            Stmt::Select { arms, .. } => {
                // Go select, 1:1 — Hybrid's arm spelling
                // (`v, ok := <-sq:` / `<-deadline:` / `ch <- v:`) lowers to
                // Go's case clauses.
                self.push("select {");
                self.indent += 1;
                for arm in arms {
                    match &arm.kind {
                        SelectArmKind::Recv { names, chan } => {
                            let c = self.emit_expr(chan, 0)?;
                            let head = match names.len() {
                                0 => format!("case <-{}:", c),
                                1 => format!("case {} := <-{}:", names[0], c),
                                2 => format!("case {}, {} := <-{}:", names[0], names[1], c),
                                n => {
                                    return Err(EmitError::unsupported(format!(
                                        "select receive with {} names",
                                        n
                                    )));
                                }
                            };
                            self.push(&head);
                        }
                        SelectArmKind::Send { chan, value } => {
                            let c = self.emit_expr(chan, 0)?;
                            let v = self.emit_expr(value, 0)?;
                            self.push(&format!("case {} <- {}:", c, v));
                        }
                        SelectArmKind::Default => self.push("default:"),
                    }
                    self.emit_block_body(&arm.body.stmts)?;
                }
                self.indent -= 1;
                self.push("}");
                Ok(())
            }
        }
    }

    // -----------------------------------------------------------------------
    // Local bindings (Define / Let / Var / Const) + the `?` desugar (E3)
    // -----------------------------------------------------------------------

    fn emit_local(&mut self, l: &LocalDecl) -> Result<(), EmitError> {
        match l.kind {
            LocalKind::Var => {
                let ty = l.ty.as_ref().map(|t| self.type_text(t));
                match (&ty, l.values.first()) {
                    (Some(t), None) => self.push(&format!("var {} {}", l.names.join(", "), t)),
                    (Some(t), Some(v)) => {
                        self.hoist_tries_expr(v)?;
                        let e = self.emit_expr(v, 0)?;
                        self.push(&format!("var {} {} = {}", l.names.join(", "), t, e));
                    }
                    (None, Some(v)) => {
                        self.hoist_tries_expr(v)?;
                        let e = self.emit_expr(v, 0)?;
                        self.push(&format!("var {} = {}", l.names.join(", "), e));
                    }
                    (None, None) => {
                        return Err(EmitError::unsupported(
                            "`var x` without a type or initializer",
                        ));
                    }
                }
                // Typed var decls carry receiver knowledge (Batch I):
                // `var hits atomic.int64` → shape-known, not a pointer.
                if let Some(t) = &l.ty {
                    if let Some(kind) = self.binding_kind_of_ty(t) {
                        for n in &l.names {
                            self.bindings.insert(n.clone(), kind);
                        }
                    }
                    if l.names.len() == 1 {
                        self.register_var_ty(&l.names[0], t);
                    }
                }
                self.hint_from_values(&l.names, &l.values);
                return Ok(());
            }
            LocalKind::Const => {
                let first = l
                    .values
                    .first()
                    .ok_or_else(|| EmitError::unsupported("`const` without an initializer"))?;
                let value = self.emit_expr(first, 0)?;
                let composite = matches!(
                    first,
                    Expr::SliceLit { .. } | Expr::MapLit { .. } | Expr::StructLit { .. }
                );
                let kw = if composite { "var" } else { "const" };
                let names = l.names.join(", ");
                match &l.ty {
                    Some(t) => {
                        let ty = self.type_text(t);
                        self.push(&format!("{} {} {} = {}", kw, names, ty, value));
                    }
                    None => self.push(&format!("{} {} = {}", kw, names, value)),
                }
                return Ok(());
            }
            // `:=` and `let` both render as Go's short declaration —
            // immutability is a checker-level contract (D14 folded
            // `let x := e` into this shape).
            LocalKind::Define | LocalKind::Let => {}
        }

        // `x := &m[k]` / `x := &s.field` — the O8 map-slot loan. Go cannot
        // take a map-index address; the ownership model's answer (O9: Go-heap
        // reads are copies) lowers to copy-read now + store-back at the
        // declaring block's end. A plain `&x` (local var address) is legal
        // Go and stays verbatim.
        if l.values.len() == 1 {
            if let Expr::AddrOf { operand, .. } = &l.values[0] {
                if matches!(operand.as_ref(), Expr::Index { .. } | Expr::Field { .. }) {
                    let place = self.emit_expr(operand, 0)?;
                    let names = l.names.join(", ");
                    self.push(&format!("{} := {}", names, place));
                    if l.names.len() == 1 {
                        if let Some(frame) = self.loan_frames.last_mut() {
                            frame.push((l.names[0].clone(), place));
                        }
                        self.bindings.insert(l.names[0].clone(), BindingKind::Plain);
                    }
                    return Ok(());
                }
            }
        }

        // `x := f()?` — the try desugar. Values.len() == 1, a Try node.
        if l.values.len() == 1 {
            if let Expr::Try { operand, .. } = &l.values[0] {
                return self.emit_try_define(l, operand);
            }
        }

        // E3 expression form (Batch G): `?` nested anywhere else in the
        // values hoists to temp bindings + propagation, then substitutes.
        for (idx, v) in l.values.iter().enumerate() {
            let later_has_try = l.values[idx + 1..]
                .iter()
                .any(Self::expr_has_hoistable_try);
            if later_has_try {
                self.hoist_order_before_try(v)?;
            } else {
                self.hoist_tries_expr(v)?;
            }
        }

        let values = l
            .values
            .iter()
            .map(|v| self.emit_expr(v, 0))
            .collect::<Result<Vec<_>, _>>()?;
        let names = l.names.join(", ");
        match (l.ty.as_ref(), values.len()) {
            (Some(_), _) => {
                return Err(EmitError::unsupported(
                    "type-ascribed `:=` — write `var x T = e` or `x := e`",
                ));
            }
            (None, _) => self.push(&format!("{} := {}", names, values.join(", "))),
        }
        // A struct-literal define carries the receiver's struct name
        // (Batch I): `stats := pool_stats{}` — field chains type through
        // struct_fields (18's `stats.mu.lock()`).
        if l.names.len() == 1 {
            if let Some(Expr::StructLit { ty: Some(t), .. }) = l.values.first() {
                self.register_var_ty(&l.names[0], t);
            }
        }
        self.hint_from_values(&l.names, &l.values);
        Ok(())
    }

    pub(crate) fn expr_key(e: &Expr) -> usize {
        e as *const Expr as usize
    }

    fn peel_parens(mut e: &Expr) -> &Expr {
        while let Expr::Paren { inner, .. } = e {
            e = inner;
        }
        e
    }

    /// True if `e` contains a `?` in the current lexical scope (does not
    /// descend into `Expr::FnLit`, which hoists its own tries).
    pub(crate) fn expr_has_hoistable_try(e: &Expr) -> bool {
        match e {
            Expr::Try { .. } => true,
            Expr::Binary { lhs, rhs, .. } => {
                Self::expr_has_hoistable_try(lhs) || Self::expr_has_hoistable_try(rhs)
            }
            Expr::Unary { operand, .. }
            | Expr::AddrOf { operand, .. }
            | Expr::Move { operand, .. }
            | Expr::Paren { inner: operand, .. }
            | Expr::Field { base: operand, .. }
            | Expr::Receive { chan: operand, .. } => Self::expr_has_hoistable_try(operand),
            Expr::Interp { parts, .. } => parts.iter().any(|p| match p {
                InterpPart::Expr(x) => Self::expr_has_hoistable_try(x),
                InterpPart::Lit(_) => false,
            }),
            Expr::Call { callee, args, .. } => {
                Self::expr_has_hoistable_try(callee)
                    || args.iter().any(|a| Self::expr_has_hoistable_try(&a.value))
            }
            Expr::Index { base, index, .. } => {
                Self::expr_has_hoistable_try(base) || Self::expr_has_hoistable_try(index)
            }
            Expr::Slice { base, lo, hi, .. } => {
                Self::expr_has_hoistable_try(base)
                    || lo
                        .as_deref()
                        .map(Self::expr_has_hoistable_try)
                        .unwrap_or(false)
                    || hi
                        .as_deref()
                        .map(Self::expr_has_hoistable_try)
                        .unwrap_or(false)
            }
            Expr::StructLit { fields, .. } => {
                fields.iter().any(|f| Self::expr_has_hoistable_try(&f.value))
            }
            Expr::SliceLit { items, .. } => items.iter().any(Self::expr_has_hoistable_try),
            Expr::MapLit { entries, .. } => entries
                .iter()
                .any(|(k, v)| Self::expr_has_hoistable_try(k) || Self::expr_has_hoistable_try(v)),
            Expr::FnLit { .. } | Expr::Lit { .. } | Expr::Ident { .. } => false,
        }
    }

    pub(crate) fn stmt_has_hoistable_try(s: &Stmt) -> bool {
        match s {
            Stmt::Local(l) => l.values.iter().any(Self::expr_has_hoistable_try),
            Stmt::Assign {
                targets, values, ..
            } => {
                targets.iter().any(Self::place_has_hoistable_try)
                    || values.iter().any(Self::expr_has_hoistable_try)
            }
            Stmt::Expr { expr, .. } => Self::expr_has_hoistable_try(expr),
            _ => false,
        }
    }

    fn place_has_hoistable_try(p: &Place) -> bool {
        match &p.kind {
            PlaceKind::Ident(_) => false,
            PlaceKind::Index { base, index } => {
                Self::place_has_hoistable_try(base) || Self::expr_has_hoistable_try(index)
            }
            PlaceKind::Field { base, .. } | PlaceKind::AddrOf { base } => {
                Self::place_has_hoistable_try(base)
            }
        }
    }

    fn expr_has_call_or_recv(e: &Expr) -> bool {
        match e {
            Expr::Call { .. } | Expr::Receive { .. } | Expr::Try { .. } => true,
            Expr::Binary { lhs, rhs, .. } => {
                Self::expr_has_call_or_recv(lhs) || Self::expr_has_call_or_recv(rhs)
            }
            Expr::Unary { operand, .. }
            | Expr::AddrOf { operand, .. }
            | Expr::Move { operand, .. }
            | Expr::Paren { inner: operand, .. }
            | Expr::Field { base: operand, .. } => Self::expr_has_call_or_recv(operand),
            Expr::Interp { parts, .. } => parts.iter().any(|p| match p {
                InterpPart::Expr(x) => Self::expr_has_call_or_recv(x),
                InterpPart::Lit(_) => false,
            }),
            Expr::Index { base, index, .. } => {
                Self::expr_has_call_or_recv(base) || Self::expr_has_call_or_recv(index)
            }
            Expr::Slice { base, lo, hi, .. } => {
                Self::expr_has_call_or_recv(base)
                    || lo
                        .as_deref()
                        .map(Self::expr_has_call_or_recv)
                        .unwrap_or(false)
                    || hi
                        .as_deref()
                        .map(Self::expr_has_call_or_recv)
                        .unwrap_or(false)
            }
            Expr::StructLit { fields, .. } => {
                fields.iter().any(|f| Self::expr_has_call_or_recv(&f.value))
            }
            Expr::SliceLit { items, .. } => items.iter().any(Self::expr_has_call_or_recv),
            Expr::MapLit { entries, .. } => entries
                .iter()
                .any(|(k, v)| Self::expr_has_call_or_recv(k) || Self::expr_has_call_or_recv(v)),
            Expr::FnLit { .. } | Expr::Lit { .. } | Expr::Ident { .. } => false,
        }
    }

    /// Preserve left-to-right operand evaluation order when a later sibling
    /// operand contains a hoisted `?`.
    fn hoist_order_before_try(&mut self, e: &Expr) -> Result<(), EmitError> {
        self.hoist_tries_expr(e)?;
        let inner = Self::peel_parens(e);
        if matches!(inner, Expr::Try { .. })
            || self.expr_temps.contains_key(&Self::expr_key(e))
            || self.expr_temps.contains_key(&Self::expr_key(inner))
        {
            return Ok(());
        }
        if Self::expr_has_call_or_recv(inner) {
            let text = self.emit_expr(e, 0)?;
            let temp = self.next_temp("hyT");
            self.push(&format!("{} := {}", temp, text));
            self.expr_temps.insert(Self::expr_key(e), temp);
        }
        Ok(())
    }

    /// E3 desugaring, wrapper-aware:
    ///
    /// ```text
    /// x := f()?
    /// ```
    /// becomes (inside the main wrapper closure — `hyErr` is its named
    /// error result; unique temps keep `:=` always-fresh at any depth):
    ///
    /// ```go
    /// x, hyErr0 := f()
    /// if hyErr0 != nil {
    ///     hyErr = hyErr0
    ///     return
    /// }
    /// ```
    /// E3's EXPRESSION form (example 05): `append(names, u.string("name")?)`
    /// or `if u.int("logins")? >= 3` — a `?` nested inside a larger
    /// expression hoists to a temp binding + propagation, then the temp
    /// substitutes at the use site (the E3 desugar, applied
    /// expression-wise; unique temps keep nesting sound).
    fn hoist_tries_expr(&mut self, e: &Expr) -> Result<(), EmitError> {
        match e {
            Expr::Try { operand, span } => {
                // Inner tries hoist first (their temps land in the map),
                // then this try's call text resolves them.
                self.hoist_tries_expr(operand)?;
                self.hoist_one_try(operand, (span.start.line, span.start.col))?;
            }
            Expr::Binary { op, lhs, rhs, .. } => {
                if matches!(op, BinOp::And | BinOp::Or) && Self::expr_has_hoistable_try(rhs) {
                    self.hoist_tries_expr(lhs)?;
                    let lhs_text = self.emit_expr(lhs, 0)?;
                    let temp = self.next_temp("hyT");
                    self.push(&format!("{} := {}", temp, lhs_text));
                    let cond_head = match op {
                        BinOp::And => format!("if {} {{", temp),
                        BinOp::Or => format!("if !{} {{", temp),
                        _ => unreachable!(),
                    };
                    self.push(&cond_head);
                    self.indent += 1;
                    self.hoist_tries_expr(rhs)?;
                    let rhs_text = self.emit_expr(rhs, 0)?;
                    self.push(&format!("{} = {}", temp, rhs_text));
                    self.indent -= 1;
                    self.push("}");
                    self.expr_temps.insert(Self::expr_key(e), temp);
                } else {
                    if Self::expr_has_hoistable_try(rhs) {
                        self.hoist_order_before_try(lhs)?;
                    } else {
                        self.hoist_tries_expr(lhs)?;
                    }
                    self.hoist_tries_expr(rhs)?;
                }
            }
            Expr::Unary { operand, .. }
            | Expr::AddrOf { operand, .. }
            | Expr::Move { operand, .. }
            | Expr::Paren { inner: operand, .. } => self.hoist_tries_expr(operand)?,
            Expr::Interp { parts, .. } => {
                for (idx, p) in parts.iter().enumerate() {
                    if let InterpPart::Expr(x) = p {
                        let later_has_try = parts[idx + 1..].iter().any(|rest| {
                            matches!(rest, InterpPart::Expr(rx) if Self::expr_has_hoistable_try(rx))
                        });
                        if later_has_try {
                            self.hoist_order_before_try(x)?;
                        } else {
                            self.hoist_tries_expr(x)?;
                        }
                    }
                }
            }
            Expr::Call { callee, args, .. } => {
                let any_arg_has_try = args.iter().any(|a| Self::expr_has_hoistable_try(&a.value));
                if any_arg_has_try {
                    match callee.as_ref() {
                        Expr::Field { base, .. } => self.hoist_order_before_try(base)?,
                        Expr::Call { .. } => self.hoist_order_before_try(callee)?,
                        _ => self.hoist_tries_expr(callee)?,
                    }
                } else {
                    self.hoist_tries_expr(callee)?;
                }
                for (idx, a) in args.iter().enumerate() {
                    let later_has_try = args[idx + 1..]
                        .iter()
                        .any(|rest| Self::expr_has_hoistable_try(&rest.value));
                    if later_has_try {
                        self.hoist_order_before_try(&a.value)?;
                    } else {
                        self.hoist_tries_expr(&a.value)?;
                    }
                }
            }
            Expr::Field { base, .. } => self.hoist_tries_expr(base)?,
            Expr::Index { base, index, .. } => {
                if Self::expr_has_hoistable_try(index) {
                    self.hoist_order_before_try(base)?;
                } else {
                    self.hoist_tries_expr(base)?;
                }
                self.hoist_tries_expr(index)?;
            }
            Expr::Slice { base, lo, hi, .. } => {
                let lo_try = lo
                    .as_deref()
                    .map(Self::expr_has_hoistable_try)
                    .unwrap_or(false);
                let hi_try = hi
                    .as_deref()
                    .map(Self::expr_has_hoistable_try)
                    .unwrap_or(false);
                if lo_try || hi_try {
                    self.hoist_order_before_try(base)?;
                } else {
                    self.hoist_tries_expr(base)?;
                }
                if let Some(l) = lo {
                    if hi_try {
                        self.hoist_order_before_try(l)?;
                    } else {
                        self.hoist_tries_expr(l)?;
                    }
                }
                if let Some(h) = hi {
                    self.hoist_tries_expr(h)?;
                }
            }
            Expr::Receive { chan, .. } => self.hoist_tries_expr(chan)?,
            Expr::StructLit { fields, .. } => {
                for (idx, f) in fields.iter().enumerate() {
                    let later_has_try = fields[idx + 1..]
                        .iter()
                        .any(|rest| Self::expr_has_hoistable_try(&rest.value));
                    if later_has_try {
                        self.hoist_order_before_try(&f.value)?;
                    } else {
                        self.hoist_tries_expr(&f.value)?;
                    }
                }
            }
            Expr::SliceLit { items, .. } => {
                for (idx, i) in items.iter().enumerate() {
                    let later_has_try = items[idx + 1..].iter().any(Self::expr_has_hoistable_try);
                    if later_has_try {
                        self.hoist_order_before_try(i)?;
                    } else {
                        self.hoist_tries_expr(i)?;
                    }
                }
            }
            Expr::MapLit { entries, .. } => {
                for (idx, (k, v)) in entries.iter().enumerate() {
                    let later_entry_try = entries[idx + 1..].iter().any(|(rk, rv)| {
                        Self::expr_has_hoistable_try(rk) || Self::expr_has_hoistable_try(rv)
                    });
                    if Self::expr_has_hoistable_try(v) || later_entry_try {
                        self.hoist_order_before_try(k)?;
                    } else {
                        self.hoist_tries_expr(k)?;
                    }
                    if later_entry_try {
                        self.hoist_order_before_try(v)?;
                    } else {
                        self.hoist_tries_expr(v)?;
                    }
                }
            }
            // A closure body hoists its OWN tries (its emission runs its
            // statement loop); descending here would hoist into the wrong
            // scope.
            Expr::FnLit { .. } | Expr::Lit { .. } | Expr::Ident { .. } => {}
        }
        Ok(())
    }

    /// One hoisted `t, hyErrN := <call>` + propagation; the temp registers
    /// under the Try's span start so emit_expr's Try branch substitutes it.
    /// The call's RESULT SHAPE (Batch I) decides the form: a 0-result call
    /// (`doc.set(k, v)?` — Set returns only error) emits `hyErrN := call`
    /// with no value temp; the Try substitutes to nothing (the statement
    /// caller skips its re-emit).
    fn hoist_one_try(&mut self, operand: &Expr, key: (u32, u32)) -> Result<(), EmitError> {
        let shape = self.call_results(operand);
        let call_text = self.emit_expr(operand, 0)?;
        if shape == Some((0, true)) {
            let errt = self.next_temp("hyErr");
            self.push(&format!("{} := {}", errt, call_text));
            self.push(&format!("if {} != nil {{", errt));
            self.indent += 1;
            self.propagate_error(&errt)?;
            self.indent -= 1;
            self.push("}");
            // Empty-string sentinel: the Try branch substitutes to nothing.
            self.try_temps.insert(key, String::new());
            return Ok(());
        }
        let temp = self.next_temp("hyT");
        let errt = self.next_temp("hyErr");
        self.push(&format!("{}, {} := {}", temp, errt, call_text));
        self.push(&format!("if {} != nil {{", errt));
        self.indent += 1;
        self.propagate_error(&errt)?;
        self.indent -= 1;
        self.push("}");
        // A serde_json Array() call yields []jsonbinding.Value — the value
        // (a for-loop var over this temp) binds the ELEMENT handle type.
        if let Expr::Call { callee, .. } = operand {
            if let Expr::Field { base, name, .. } = callee.as_ref() {
                if name == "array" && self.type_of_expr(base) == Some("serde_json.value") {
                    self.bindings
                        .insert(temp.clone(), BindingKind::ForeignTy("serde_json.value"));
                }
            }
        }
        self.try_temps.insert(key, temp);
        Ok(())
    }

    /// The result shape of a call expression: (n_results, has_error).
    /// Known from four sources (Batch I): rust-crate descriptors, typed
    /// binding methods, user-fn declarations, and the default rust-call
    /// convention (1, true). None = unknown → callers keep today's shape.
    fn call_results(&self, e: &Expr) -> Option<(usize, bool)> {
        let callee = match e {
            Expr::Try { operand, .. } => match operand.as_ref() {
                Expr::Call { callee, .. } => callee.as_ref(),
                _ => return None,
            },
            Expr::Call { callee, .. } => callee.as_ref(),
            _ => return None,
        };
        match callee {
            // rust-crate calls: the descriptor's results field.
            Expr::Field { base, name, .. } if matches!(base.as_ref(), Expr::Ident { .. }) => {
                if let Expr::Ident { name: pkg, .. } = base.as_ref() {
                    if is_package_root(pkg) {
                        if let Some(desc) = super::shapes::lookup_rust_crate(pkg) {
                            return desc
                                .fns
                                .iter()
                                .find(|f| f.hy == name.as_str())
                                .map(|f| (f.results.0 as usize, f.results.1));
                        }
                        return None;
                    }
                }
                // Typed binding methods: the METHOD_RESULTS table.
                if let Some(bt) = self.type_of_expr(base) {
                    if let Some(m) = super::shapes::lookup_member(bt, name) {
                        if m.kind == super::shapes::MemberKind::Method {
                            let (n, e) =
                                super::shapes::lookup_method_results(bt, name).unwrap_or((1, true));
                            return Some((n as usize, e));
                        }
                    }
                }
                None
            }
            // User fns: the eagerly collected declared shape.
            Expr::Ident { name, .. } => self
                .fn_shapes
                .get(name.as_str())
                .map(|(n, err, _, _)| (*n, *err)),
            _ => None,
        }
    }

    /// The `?` propagation tail, shared by every E3 shape (binding-position,
    /// expression-form hoists, the httptest composite): the E11 handler
    /// contract, the E6 main-closure return, an error-result fn's
    /// zero-results return (package mode), or a clean error.
    fn propagate_error(&mut self, err_temp: &str) -> Result<(), EmitError> {
        if let Some(w) = self.handler_writer.clone() {
            self.use_package("http");
            self.push(&format!(
                "http.Error({}, {}.Error(), http.StatusBadRequest)",
                w, err_temp
            ));
            self.push("return");
            return Ok(());
        }
        if self.in_main_closure {
            self.push(&format!("hyErr = {}", err_temp));
            self.push("return");
            return Ok(());
        }
        if let Some(mut rets) = self.cur_fn_error_returns.clone() {
            let n = rets.len() - 1;
            rets[n] = err_temp.to_string();
            self.push(&format!("return {}", rets.join(", ")));
            return Ok(());
        }
        Err(EmitError::unsupported(
            "`?` outside main (functions-with-error-results land after the walking skeleton)",
        ))
    }

    fn emit_try_define(&mut self, l: &LocalDecl, call: &Expr) -> Result<(), EmitError> {
        // --- the httptest composite (Batch E): `code, body :=
        //     httptest.get(mux, path)?` expands to the in-process recorder
        //     dance. The only error source is NewRequest; the `?` shape
        //     (E6 main closure vs E11 handler) governs its propagation. ---
        if let Expr::Call {
            callee,
            args,
            span: _,
        } = call
        {
            if let Expr::Field {
                base,
                name,
                span: _,
            } = callee.as_ref()
            {
                if let Expr::Ident { name: pkg, .. } = base.as_ref() {
                    if *pkg == "httptest" && (*name == "get" || *name == "post") {
                        return self.emit_httptest_get(l, args, name);
                    }
                }
            }
        }

        // --- E11 handler contract: `?` inside a closure converting to a
        //     propagation-carrying target. The target's signature has no
        //     error result, so E3's error-return cannot land; the target's
        //     declared propagation (http.Error on the writer) is the
        //     contract instead. ---
        if let Some(w) = self.handler_writer.clone() {
            self.hoist_tries_expr(call)?;
            let temp = self.next_temp("hyErr");
            let call_text = self.emit_expr(call, 0)?;
            let mut names = l.names.clone();
            names.push(temp.clone());
            self.push(&format!("{} := {}", names.join(", "), call_text));
            self.push(&format!("if {} != nil {{", temp));
            self.indent += 1;
            self.use_package("http");
            self.push(&format!(
                "http.Error({}, {}.Error(), http.StatusBadRequest)",
                w, temp
            ));
            self.push("return");
            self.indent -= 1;
            self.push("}");
            self.hint_from_values(&l.names, std::slice::from_ref(call));
            return Ok(());
        }

        if !self.in_main_closure && self.cur_fn_error_returns.is_none() {
            return Err(EmitError::unsupported(
                "`?` outside main (functions-with-error-results land after the walking skeleton)",
            ));
        }
        self.hoist_tries_expr(call)?;
        let temp = self.next_temp("hyErr");
        let call_text = self.emit_expr(call, 0)?;
        let mut names = l.names.clone();
        names.push(temp.clone());
        self.push(&format!("{} := {}", names.join(", "), call_text));
        self.push(&format!("if {} != nil {{", temp));
        self.indent += 1;
        self.propagate_error(&temp)?;
        self.indent -= 1;
        self.push("}");
        self.hint_from_values(&l.names, std::slice::from_ref(call));
        Ok(())
    }

    /// `code, body := httptest.get(mux, path)?` — the in-process smoke
    /// client (example 06). The stdlib idiom, spelled out honestly:
    ///
    /// ```go
    /// hyReq0, hyErr0 := http.NewRequest("GET", path, nil)
    /// if hyErr0 != nil { <propagate> }
    /// hyRec0 := httptest.NewRecorder()
    /// mux.ServeHTTP(hyRec0, hyReq0)
    /// code, body := hyRec0.Code, hyRec0.Body.String()
    /// ```
    fn emit_httptest_get(
        &mut self,
        l: &LocalDecl,
        args: &[CallArg],
        verb: &str,
    ) -> Result<(), EmitError> {
        if args.len() != 2 {
            return Err(EmitError::unsupported(format!(
                "httptest.{} takes (handler, path)",
                verb
            )));
        }
        if l.names.len() != 2 {
            return Err(EmitError::unsupported(
                "httptest.get/post binds exactly two names: `code, body := httptest.get(…)?`",
            ));
        }
        let handler = self.emit_expr(&args[0].value, ATOM)?;
        let path = self.emit_expr(&args[1].value, 0)?;
        let req = self.next_temp("hyReq");
        let err0 = self.next_temp("hyErr");
        let rec = self.next_temp("hyRec");
        self.use_package("http");
        self.use_package("httptest");
        self.push(&format!(
            "{}, {} := http.NewRequest(\"{}\", {}, nil)",
            req,
            err0,
            verb.to_uppercase(),
            path
        ));
        self.push(&format!("if {} != nil {{", err0));
        self.indent += 1;
        self.propagate_error(&err0)?;
        self.indent -= 1;
        self.push("}");
        self.push(&format!("{} := httptest.NewRecorder()", rec));
        self.push(&format!("{}.ServeHTTP({}, {})", handler, rec, req));
        // The smoke client returns the body LINE: the response's single
        // trailing newline is trimmed, so `fmt.println("{code} {body}")`
        // prints one line per response (example 06's expected output pins
        // this — the handler's own Fprintln newline is transport, not
        // payload). One TrimSuffix — deterministic, never a regexp.
        self.use_package("strings");
        self.push(&format!(
            "{} := {}.Code, strings.TrimSuffix({}.Body.String(), \"\\n\")",
            l.names.join(", "),
            rec,
            rec
        ));
        // The recorder path carries no error — hints ride on the handler.
        Ok(())
    }

    /// Binding hints from the defining values (see `BindingKind`).
    fn hint_from_values(&mut self, names: &[String], values: &[Expr]) {
        if names.len() != 1 || values.len() != 1 {
            return;
        }
        let kind = self.binding_kind_of(&values[0]);
        if kind != BindingKind::Plain {
            self.bindings.insert(names[0].clone(), kind);
        }
    }

    fn binding_kind_of(&self, e: &Expr) -> BindingKind {
        let call = match e {
            Expr::Try { operand, .. } => match operand.as_ref() {
                Expr::Call { callee, .. } => callee.as_ref(),
                _ => return BindingKind::Plain,
            },
            Expr::Call { callee, .. } => callee.as_ref(),
            _ => return BindingKind::Plain,
        };
        if let Expr::Field { base, name, .. } = call {
            if let Expr::Ident { name: pkg, .. } = base.as_ref() {
                if is_package_root(pkg) {
                    if pkg == "io" && name == "read_all" {
                        return BindingKind::Bytes;
                    }
                    // Rust-handle constructors: a serde_json value handle.
                    if pkg == "serde_json"
                        && matches!(name.as_str(), "parse" | "new_object" | "new_array")
                    {
                        return BindingKind::ForeignTy("serde_json.value");
                    }
                    // time.since/now return a KNOWN type — duration members
                    // resolve property-style through the shape table (10:
                    // `dt.milliseconds` → `dt.Milliseconds()`).
                    if pkg == "time" && name == "since" {
                        return BindingKind::ForeignTy("time.duration");
                    }
                    if pkg == "time" && name == "now" {
                        return BindingKind::ForeignTy("time.time");
                    }
                    return BindingKind::Foreign;
                }
            }
        }
        // A user fn whose declared result type is a known handle type: the
        // binding resolves through the shape table (Batch I, corpus 13:
        // `s := summary(doc)?` — s.set normalizes like any rust-call
        // result).
        if let Expr::Ident { name, .. } = call {
            if let Some((_, _, Some(kind), _)) = self.fn_shapes.get(name.as_str()) {
                return *kind;
            }
        }
        // make(chan …) → Chan (single-var range emits the value directly).
        if matches!(call, Expr::Ident { name, .. } if name == "make") {
            // args[0] is the type-as-value encoding; peek at its shape.
            // (Verified against the parser's make encoding.)
            return BindingKind::Chan;
        }
        BindingKind::Plain
    }

    // -----------------------------------------------------------------------
    // Assignment
    // -----------------------------------------------------------------------

    fn hoist_tries_place(&mut self, p: &Place, later_has_try: bool) -> Result<(), EmitError> {
        match &p.kind {
            PlaceKind::Ident(_) => Ok(()),
            PlaceKind::Index { base, index } => {
                let idx_try = Self::expr_has_hoistable_try(index);
                self.hoist_tries_place(base, idx_try || later_has_try)?;
                if later_has_try {
                    self.hoist_order_before_try(index)?;
                } else {
                    self.hoist_tries_expr(index)?;
                }
                Ok(())
            }
            PlaceKind::Field { base, .. } | PlaceKind::AddrOf { base } => {
                self.hoist_tries_place(base, later_has_try)
            }
        }
    }

    fn emit_assign(
        &mut self,
        targets: &[Place],
        op: AssignOp,
        values: &[Expr],
    ) -> Result<(), EmitError> {
        // `x := &m[k]` / `x := &s.field` — the O8 map-slot loan. Go cannot
        // take a map-index address; the ownership model's answer (O9: Go-heap
        // reads are copies) lowers to copy-read now + store-back at the
        // declaring block's end. A plain `&x` (local var address) is legal
        // Go and stays verbatim.
        if targets.len() == 1 && values.len() == 1 && op == AssignOp::Assign {
            if let Expr::AddrOf { operand, .. } = &values[0] {
                if matches!(operand.as_ref(), Expr::Index { .. } | Expr::Field { .. }) {
                    if let PlaceKind::Ident(name) = &targets[0].kind {
                        let place = self.emit_expr(operand, 0)?;
                        self.push(&format!("{} = {}", name, place));
                        if let Some(frame) = self.loan_frames.last_mut() {
                            frame.push((name.clone(), place));
                        }
                        return Ok(());
                    }
                }
            }
        }

        let any_val_try = values.iter().any(Self::expr_has_hoistable_try);
        for (idx, t) in targets.iter().enumerate() {
            let later_target_try = targets[idx + 1..].iter().any(Self::place_has_hoistable_try);
            self.hoist_tries_place(t, later_target_try || any_val_try)?;
        }
        let op_text = match op {
            AssignOp::Assign => "=",
            AssignOp::AddAssign => "+=",
            AssignOp::SubAssign => "-=",
            AssignOp::MulAssign => "*=",
            AssignOp::DivAssign => "/=",
            AssignOp::ModAssign => "%=",
            AssignOp::BitOrAssign => "|=",
            AssignOp::BitAndAssign => "&=",
            AssignOp::BitXorAssign => "^=",
            AssignOp::ShlAssign => "<<=",
            AssignOp::ShrAssign => ">>=",
            AssignOp::Inc => {
                let t = self.emit_place(&targets[0])?;
                self.push(&format!("{}++", t));
                return Ok(());
            }
            AssignOp::Dec => {
                let t = self.emit_place(&targets[0])?;
                self.push(&format!("{}--", t));
                return Ok(());
            }
        };
        // E3 expression form: hoist nested `?` before emitting (Batch G).
        for (idx, v) in values.iter().enumerate() {
            let later_has_try = values[idx + 1..].iter().any(Self::expr_has_hoistable_try);
            if later_has_try {
                self.hoist_order_before_try(v)?;
            } else {
                self.hoist_tries_expr(v)?;
            }
        }
        let ts = targets
            .iter()
            .map(|t| self.emit_place(t))
            .collect::<Result<Vec<_>, _>>()?;
        let vs = values
            .iter()
            .map(|v| self.emit_expr(v, 0))
            .collect::<Result<Vec<_>, _>>()?;
        self.push(&format!("{} {} {}", ts.join(", "), op_text, vs.join(", ")));
        Ok(())
    }

    /// A place in Go spelling. Field members on Foreign bindings normalize
    /// (M4); our own struct members stay verbatim.
    pub fn emit_place(&mut self, p: &Place) -> Result<String, EmitError> {
        match &p.kind {
            PlaceKind::Ident(n) => Ok(n.clone()),
            PlaceKind::Index { base, index } => {
                let b = self.emit_place(base)?;
                let i = self.emit_expr(index, 0)?;
                Ok(format!("{}[{}]", b, i))
            }
            PlaceKind::Field { base, name } => {
                let b = self.emit_place(base)?;
                Ok(format!("{}.{}", b, self.place_member_name(base, name)))
            }
            PlaceKind::AddrOf { base } => {
                let b = self.emit_place(base)?;
                Ok(format!("&{}", b))
            }
        }
    }

    /// Member-name normalization for a PLACE (assignment target). Same rule
    /// as the expression side: foreign package roots and Foreign bindings
    /// normalize; our own members stay verbatim.
    pub fn place_member_name(&self, base: &Place, name: &str) -> String {
        let root = base.root();
        if is_package_root(root)
            || matches!(
                self.bindings.get(root),
                Some(BindingKind::Foreign) | Some(BindingKind::ForeignTy(_))
            )
        {
            return super::to_go_name(name);
        }
        name.to_string()
    }

    /// Member-name normalization decision for a field/method access whose
    /// base's ROOT binding is `base` (an Ident chain root).
    pub fn member_name(&self, base: &Expr, name: &str) -> String {
        if let Expr::Ident { name: root, .. } = base {
            if is_package_root(root) {
                return to_go_name(name);
            }
            if matches!(
                self.bindings.get(root),
                Some(BindingKind::Foreign) | Some(BindingKind::ForeignTy(_))
            ) {
                return to_go_name(name);
            }
        }
        name.to_string()
    }

    // -----------------------------------------------------------------------
    // Control flow
    // -----------------------------------------------------------------------

    fn emit_if(
        &mut self,
        init: Option<&Stmt>,
        cond: &Expr,
        then: &Block,
        otherwise: Option<&Stmt>,
    ) -> Result<(), EmitError> {
        // When `if <init>; <cond>` has a `?` in either `<init>` or `<cond>`,
        // lower to a scoped block `{ <init>; <hoist(cond)>; if <cond> { … } }`
        // so bindings introduced by `<init>` are in scope for `<cond>`'s
        // hoisted statements and remain scoped to the if/else construct.
        if let Some(i) = init {
            if Self::stmt_has_hoistable_try(i) || Self::expr_has_hoistable_try(cond) {
                self.push("{");
                self.loan_frames.push(Vec::new());
                self.indent += 1;
                self.emit_stmt(i)?;
                self.hoist_tries_expr(cond)?;
                self.emit_if_at(None, cond, then, otherwise, true)?;
                let frame = self.loan_frames.pop().unwrap_or_default();
                for (name, place) in frame {
                    self.push(&format!("{} = {}", place, name));
                }
                self.indent -= 1;
                self.push("}");
                return Ok(());
            }
        }
        // E3 expression form: a `?` in the condition hoists BEFORE the if
        // (Batch G).
        self.hoist_tries_expr(cond)?;
        self.emit_if_at(init, cond, then, otherwise, true)
    }

    /// `at_line_start=false` renders the head as a `} else if …` CONTINUATION
    /// on the current line (else-if chains), never a fresh indented line.
    fn emit_if_at(
        &mut self,
        init: Option<&Stmt>,
        cond: &Expr,
        then: &Block,
        otherwise: Option<&Stmt>,
        at_line_start: bool,
    ) -> Result<(), EmitError> {
        let mut head = String::new();
        if at_line_start {
            for _ in 0..self.indent {
                head.push('\t');
            }
            head.push_str("if ");
        } else {
            head.push_str("if ");
        }
        if let Some(i) = init {
            // Go: `if <stmt>; <cond> {` — the init renders without newline.
            let init_text = self.stmt_inline(i)?;
            head.push_str(&init_text);
            head.push_str("; ");
        }
        let c = self.emit_expr(cond, 0)?;
        head.push_str(&c);
        head.push_str(" {");
        self.push_raw(&head);
        self.push_raw("\n");
        self.emit_block_body(&then.stmts)?;
        match otherwise {
            None => self.push("}"),
            Some(Stmt::If {
                init,
                cond,
                then,
                otherwise,
                ..
            }) => {
                if init
                    .as_deref()
                    .map(Self::stmt_has_hoistable_try)
                    .unwrap_or(false)
                    || Self::expr_has_hoistable_try(cond)
                {
                    self.push("} else {");
                    self.indent += 1;
                    self.emit_if(init.as_deref(), cond, then, otherwise.as_deref())?;
                    self.indent -= 1;
                    self.push("}");
                } else {
                    self.push_raw("} else ");
                    self.emit_if_at(init.as_deref(), cond, then, otherwise.as_deref(), false)?;
                }
            }
            Some(Stmt::Block { block, .. }) => {
                // `else { … }` — emit the block's statements directly (a
                // bare-block emit here would double the braces).
                self.push("} else {");
                self.emit_block_body(&block.stmts)?;
                self.push("}");
            }
            Some(other) => {
                self.push("} else {");
                self.emit_block_body(std::slice::from_ref(other))?;
                self.push("}");
            }
        }
        Ok(())
    }

    /// Inline statement rendering for if-init (`v, ok := m[k]`).
    fn stmt_inline(&mut self, s: &Stmt) -> Result<String, EmitError> {
        match s {
            Stmt::Local(l) => {
                // try-desugar cannot appear in an if-init (the init cond is
                // checked by the checker) — render plainly.
                let values = l
                    .values
                    .iter()
                    .map(|v| self.emit_expr(v, 0))
                    .collect::<Result<Vec<_>, _>>()?;
                let names = l.names.join(", ");
                Ok(match (&l.ty, values.len()) {
                    (Some(t), 1) => format!("{} := {} {}", names, self.type_text(t), values[0]),
                    _ => format!("{} := {}", names, values.join(", ")),
                })
            }
            Stmt::Expr { expr, .. } => self.emit_expr(expr, 0),
            _ => Err(EmitError::unsupported(
                "that statement form in an if-init (MVP-0 supports defines)",
            )),
        }
    }

    /// Every block body goes through this: a loan frame is pushed, the
    /// statements render, and the frame's store-backs flush at the block's
    /// end (before the closing brace) unless the block already terminated via
    /// `return`, `break`, or `continue` (which flush active loans before the
    /// control-flow transfer, #203). This is what makes the O8 slot-loan
    /// desugar sound: mutations through the loaned copy persist exactly for
    /// the loan's lexical window.
    pub fn emit_block_body(&mut self, stmts: &[Stmt]) -> Result<(), EmitError> {
        self.loan_frames.push(Vec::new());
        self.indent += 1;
        for st in stmts {
            self.emit_stmt(st)?;
        }
        let frame = self.loan_frames.pop().unwrap_or_default();
        if !stmts.iter().any(Self::stmt_terminates) {
            for (name, place) in frame {
                if !name.is_empty() {
                    self.push(&format!("{} = {}", place, name));
                }
            }
        }
        self.indent -= 1;
        Ok(())
    }

    fn block_terminates(b: &Block) -> bool {
        b.stmts.iter().any(Self::stmt_terminates)
    }

    fn stmt_terminates(s: &Stmt) -> bool {
        match s {
            Stmt::Return { .. } | Stmt::Break { .. } | Stmt::Continue { .. } => true,
            Stmt::Block { block, .. } | Stmt::Managed { body: block, .. } => {
                Self::block_terminates(block)
            }
            Stmt::If {
                then,
                otherwise: Some(else_stmt),
                ..
            } => Self::block_terminates(then) && Self::stmt_terminates(else_stmt),
            Stmt::Match { arms, .. } => {
                !arms.is_empty()
                    && arms
                        .iter()
                        .all(|a| a.guard.is_none() && Self::block_terminates(&a.body))
            }
            _ => false,
        }
    }

    fn flush_loans_for_return(&mut self) {
        let mut stores: Vec<(String, String)> = Vec::new();
        for frame in self.loan_frames.iter().rev() {
            if frame.first().map(|(n, _)| n.is_empty()).unwrap_or(false) {
                continue;
            }
            for (name, place) in frame {
                stores.push((name.clone(), place.clone()));
            }
        }
        for (name, place) in stores {
            self.push(&format!("{} = {}", place, name));
        }
    }

    fn flush_loans_for_loop(&mut self, labeled: bool) {
        let mut stores: Vec<(String, String)> = Vec::new();
        for frame in self.loan_frames.iter().rev() {
            if frame.first().map(|(n, _)| n.is_empty()).unwrap_or(false) {
                if !labeled || !frame[0].1.is_empty() {
                    break;
                }
                continue;
            }
            for (name, place) in frame {
                stores.push((name.clone(), place.clone()));
            }
        }
        for (name, place) in stores {
            self.push(&format!("{} = {}", place, name));
        }
    }

    fn emit_for(&mut self, kind: &ForKind, body: &Block) -> Result<(), EmitError> {
        // A labeled `break loop` / `continue loop` inside this loop (NOT
        // inside a nested loop — that one labels itself) makes this loop the
        // Go label target: `hyLoop:` + the for head, `break hyLoop` inside.
        let needs_label = self.block_has_loop_break(body);
        // Two sibling loops each owning a `break loop` would both declare
        // `hyLoop:` — Go rejects that (`label hyLoop already declared`), and
        // no emission can fix a duplicate declaration, so each labeled loop
        // takes a fresh name (`hyLoop`, `hyLoop1`, …). The label is pushed
        // BEFORE the body renders (so the breaks inside pick it up) and
        // pushed/pop popped around the body.
        let label = if needs_label {
            // A per-function counter, not the stack depth: sibling loops
            // unwind the stack between them, so depth would hand both the
            // same name.
            let n = self.loop_label_n;
            self.loop_label_n += 1;
            let name = if n == 0 {
                "hyLoop".to_string()
            } else {
                format!("hyLoop{}", n)
            };
            self.loop_labels.push(name.clone());
            Some(name)
        } else {
            None
        };
        // gofmt puts a loop label on its OWN line (`hyLoop:` then the for
        // head one level in) — emit it as a separate line, never inline.
        if let Some(l) = &label {
            self.push(&format!("{}:", l));
        }
        if let ForKind::While { cond } = kind {
            if Self::expr_has_hoistable_try(cond) {
                self.push("for {");
                self.loan_frames.push(Vec::new());
                self.indent += 1;
                self.hoist_tries_expr(cond)?;
                let c = self.emit_expr(cond, ATOM)?;
                self.push(&format!("if !{} {{", c));
                self.indent += 1;
                self.push("break");
                self.indent -= 1;
                self.push("}");
                for st in &body.stmts {
                    self.emit_stmt(st)?;
                }
                let frame = self.loan_frames.pop().unwrap_or_default();
                for (name, place) in frame {
                    self.push(&format!("{} = {}", place, name));
                }
                self.indent -= 1;
                self.push("}");
                return Ok(());
            }
        }
        let head = match kind {
            ForKind::Infinite => "for {".to_string(),
            ForKind::While { cond } => {
                format!("for {} {{", self.emit_expr(cond, 0)?)
            }
            ForKind::Range {
                var,
                lo,
                hi,
                inclusive,
            } => {
                if Self::expr_has_hoistable_try(hi) {
                    self.hoist_order_before_try(lo)?;
                } else {
                    self.hoist_tries_expr(lo)?;
                }
                self.hoist_tries_expr(hi)?;
                let lo_t = self.emit_expr(lo, 0)?;
                let hi_t = self.emit_expr(hi, 0)?;
                let op = if *inclusive { "<=" } else { "<" };
                // `for _ in a..b` (differential corpus 16/17/18): Go has no
                // blank := variable — a fresh temp carries the count. The
                // semantics are identical: the variable is never read.
                let v = if var == "_" {
                    self.next_temp("hyBlank")
                } else {
                    var.clone()
                };
                format!(
                    "for {} := {}; {} {} {}; {}++ {{",
                    v, lo_t, v, op, hi_t, v
                )
            }
            ForKind::Iterate { vars, expr } => {
                // E3 expression form: `for u in v.array(k)? { … }` hoists
                // the try before the loop (Batch G).
                self.hoist_tries_expr(expr)?;
                let e = self.emit_expr(expr, 0)?;
                let core = match vars.len() {
                    1 => {
                        // `for _ in ch` drains without binding (corpus 17);
                        // `for range e` is Go's blank form for every
                        // iterable. No variable to register.
                        if vars[0] == "_" {
                            format!("for range {} {{", e)
                        } else {
                            // Hybrid's single-var iterate binds the VALUE.
                            // Go's single-var range binds the index (slices/
                            // maps) or the value (channels) — hence the Chan
                            // hint check.
                            let over_chan = match expr {
                                Expr::Ident { name, .. } => {
                                    matches!(self.bindings.get(name), Some(BindingKind::Chan))
                                }
                                _ => false,
                            };
                            if over_chan {
                                format!("for {} := range {} {{", vars[0], e)
                            } else {
                                // A rust-handle slice (ForeignTy binding)
                                // ranges value-elements of that handle type
                                // (05: u is a serde_json.value). The range may
                                // be a named slice OR a hoisted try temp (E IS
                                // the temp).
                                let range_name = match expr {
                                    Expr::Ident { name, .. } => Some(name.clone()),
                                    Expr::Try { .. } => Some(e.clone()),
                                    _ => None,
                                };
                                if let Some(n) = range_name {
                                    if let Some(BindingKind::ForeignTy(t)) = self.bindings.get(&n) {
                                        let t = *t;
                                        self.bindings
                                            .insert(vars[0].clone(), BindingKind::ForeignTy(t));
                                    }
                                }
                                format!("for _, {} := range {} {{", vars[0], e)
                            }
                        }
                    }
                    2 => {
                        // Go rejects a range binding the body never reads
                        // (`declared and not used`), and `for _, _ :=` is
                        // itself invalid (`no new variables on left side of
                        // :=`) - so an unread slot lowers to `_`, and a body
                        // that reads neither binding drops the list entirely.
                        // Same lowering the one-variable form already uses.
                        // Issue #139.
                        let a = if crate::rangebind::body_reads(body, &vars[0]) {
                            vars[0].clone()
                        } else {
                            "_".to_string()
                        };
                        let c = if crate::rangebind::body_reads(body, &vars[1]) {
                            vars[1].clone()
                        } else {
                            "_".to_string()
                        };
                        if a == "_" && c == "_" {
                            format!("for range {} {{", e)
                        } else {
                            format!("for {}, {} := range {} {{", a, c, e)
                        }
                    }
                    n => {
                        return Err(EmitError::unsupported(format!(
                            "for-in with {} loop variables",
                            n
                        )))
                    }
                };
                core
            }
        };
        self.push(&head);
        self.for_depth += 1;
        let sentinel_label = label.clone().unwrap_or_default();
        self.loan_frames.push(vec![(String::new(), sentinel_label)]);
        let body_r = self.emit_block_body(&body.stmts);
        self.loan_frames.pop();
        self.for_depth -= 1;
        body_r?;
        self.push("}");
        if label.is_some() {
            self.loop_labels.pop();
        }
        Ok(())
    }

    /// Does this block contain a labeled break/continue whose nearest
    /// enclosing loop is THIS one? Does not descend into nested `for`
    /// statements (a nested loop that owns a labeled break labels itself).
    fn block_has_loop_break(&self, b: &Block) -> bool {
        b.stmts.iter().any(|s| self.stmt_has_loop_break(s))
    }

    fn stmt_has_loop_break(&self, s: &Stmt) -> bool {
        match s {
            Stmt::Break { label: Some(_), .. } | Stmt::Continue { label: Some(_), .. } => true,
            Stmt::If {
                init,
                then,
                otherwise,
                ..
            } => {
                init.as_ref()
                    .map(|i| self.stmt_has_loop_break(i))
                    .unwrap_or(false)
                    || self.block_has_loop_break(then)
                    || otherwise
                        .as_ref()
                        .map(|o| self.stmt_has_loop_break(o))
                        .unwrap_or(false)
            }
            Stmt::For { .. } => false, // nested loop labels itself
            Stmt::Block { block, .. } | Stmt::Managed { body: block, .. } => {
                self.block_has_loop_break(block)
            }
            Stmt::Match { arms, .. } => {
                // A labeled break/continue inside a match arm still targets
                // THIS loop (the arm is not a new loop) — without this arm
                // the loop skipped its `hyLoop:` label and the emitted
                // `break hyLoop` failed go vet (invalid break label).
                arms.iter().any(|a| self.block_has_loop_break(&a.body))
            }
            Stmt::Select { arms, .. } => arms.iter().any(|a| self.block_has_loop_break(&a.body)),
            _ => false,
        }
    }

    // -----------------------------------------------------------------------
    // Return (wrapper-aware)
    // -----------------------------------------------------------------------

    fn emit_return(&mut self, values: &[Expr]) -> Result<(), EmitError> {
        self.flush_loans_for_return();
        for (idx, v) in values.iter().enumerate() {
            let later_has_try = values[idx + 1..].iter().any(Self::expr_has_hoistable_try);
            if later_has_try {
                self.hoist_order_before_try(v)?;
            } else {
                self.hoist_tries_expr(v)?;
            }
        }
        // Inside a function literal the closure owns the return — plain.
        if self.fnlit_depth > 0 {
            let vs = values
                .iter()
                .map(|v| self.emit_expr(v, 0))
                .collect::<Result<Vec<_>, _>>()?;
            self.push(&format!("return {}", vs.join(", ")));
            return Ok(());
        }
        // Inside the main wrapper closure: explicit returns carry `nil` for
        // the hidden error result; bare `return` is legal (named results)
        // and returns the current hyErr — exactly the E3 desugar's shape.
        if self.in_main_closure {
            if values.is_empty() {
                self.push("return");
                return Ok(());
            }
            let vs = values
                .iter()
                .map(|v| self.emit_expr(v, 0))
                .collect::<Result<Vec<_>, _>>()?;
            self.push(&format!("return {}, nil", vs.join(", ")));
            return Ok(());
        }
        // `return fn(…)` where the fn's result is a conversion target
        // (Batch E): Go's interface needs the HandlerFunc conversion.
        if let Some(key) = self.cur_fn_conv_result {
            if values.len() == 1 && matches!(values[0], Expr::FnLit { .. }) {
                if let Some(conv) = lookup_conversion(key) {
                    self.pending_fnlit_conv = Some(conv);
                    let lit = self.emit_expr(&values[0], 0)?;
                    self.pending_fnlit_conv = None;
                    self.use_package(key.split('.').next().unwrap_or(key));
                    self.push(&format!("return {}({})", conv.go, lit));
                    return Ok(());
                }
            }
        }
        // `return none` — lower against the declared option result (the
        // prelude's zero value IS none).
        // `return _, err` — the `_` slot fills with the result's zero value
        // (SPEC E3's sugar; unnamed results cannot say `_` in Go). Only in
        // fns whose error-returns shape is known (E3-capable fns).
        let mut vs = Vec::new();
        for (i, v) in values.iter().enumerate() {
            if matches!(v, Expr::Ident { name, .. } if name == "none") {
                match &self.cur_option_result {
                    Some(opt) => vs.push(format!("{}{{}}", opt)),
                    None => {
                        return Err(EmitError::unsupported(
                            "`return none` in a function whose result is not option[T]",
                        ));
                    }
                }
            } else if matches!(v, Expr::Ident { name, .. } if name == "_") {
                match &self.cur_fn_error_returns {
                    Some(zeros) if i < zeros.len() => vs.push(zeros[i].clone()),
                    _ => {
                        return Err(EmitError::unsupported(
                            "`return _` needs a fn whose results end in `error` (E3's zero-fill)",
                        ));
                    }
                }
            } else {
                vs.push(self.emit_expr(v, 0)?);
            }
        }
        // A short return in an error-result fn fills the remaining slots
        // with their zero texts (13: `return v` in a (value, error) fn →
        // `return v, nil`) — Go demands the full arity. An EMPTY slot is
        // the error result (propagate_error fills it at propagation time);
        // a short explicit return means nil.
        if let Some(zeros) = &self.cur_fn_error_returns {
            while vs.len() < zeros.len() {
                let i = vs.len();
                let z = &zeros[i];
                vs.push(if z.is_empty() {
                    "nil".to_string()
                } else {
                    z.clone()
                });
            }
        }
        self.push(&format!("return {}", vs.join(", ")));
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Match — option-subject lowering (some(v) / none / _)
    // -----------------------------------------------------------------------

    /// `match c.get(k) { some(v): A, none: B }` lowers to
    ///
    /// ```go
    /// hyMatchN := c.get(k)
    /// if hyMatchN.ok {
    ///     v := hyMatchN.val
    ///     A
    /// } else {
    ///     B
    /// }
    /// ```
    ///
    /// General enum subjects land with the enum Go-representation decision
    /// (SPEC D-note); this surface is exactly what example 07 needs.
    fn emit_match(&mut self, subject: &Expr, arms: &[MatchArm]) -> Result<(), EmitError> {
        enum ArmKind {
            Some(String),
            NoneArm,
            Wildcard,
        }
        let mut classified: Vec<(ArmKind, &MatchArm)> = Vec::new();
        for a in arms {
            let kind = match &a.pattern {
                MatchPattern::Path { segments, binding } if segments == &["some"] => {
                    match binding {
                        Some(v) => ArmKind::Some(v.clone()),
                        None => {
                            return Err(EmitError::unsupported(
                                "`some` pattern without a binding — write `some(v):`",
                            ));
                        }
                    }
                }
                MatchPattern::Path {
                    segments,
                    binding: _,
                } if segments == &["none"] => ArmKind::NoneArm,
                MatchPattern::Wildcard => ArmKind::Wildcard,
                other => {
                    return Err(EmitError::unsupported(format!(
                        "match pattern `{:?}` — option-subject matches (some(v)/none/_) only; \
                         general enum matches land with the enum representation decision",
                        other
                    )));
                }
            };
            classified.push((kind, a));
        }
        if let Some((ArmKind::Wildcard, _)) = classified.first() {
            return Err(EmitError::unsupported(
                "`_` as the FIRST match arm (MVP-0 lowers some→if, none/_→else)",
            ));
        }

        let temp = self.next_temp("hyMatch");
        let s = self.emit_expr(subject, 0)?;
        self.push(&format!("{} := {}", temp, s));

        let mut some_arms: Vec<(&String, &MatchArm)> = Vec::new();
        let mut none_arm: Option<&MatchArm> = None;
        let mut wildcard_arm: Option<&MatchArm> = None;
        for (kind, arm) in &classified {
            match kind {
                ArmKind::Some(binding) => some_arms.push((binding, *arm)),
                ArmKind::NoneArm => none_arm = Some(*arm),
                ArmKind::Wildcard => wildcard_arm = Some(*arm),
            }
        }

        if some_arms.is_empty() {
            // No some-arm: the checker's exhaustiveness (E5001) rejects
            // this before emission; nothing to lower.
            return Ok(());
        }

        let has_unguarded_some = some_arms.iter().any(|(_, arm)| arm.guard.is_none());

        if has_unguarded_some {
            let else_arm = none_arm.or(wildcard_arm);
            if some_arms.len() == 1 {
                let (binding, arm) = some_arms[0];
                self.push(&format!("if {}.ok {{", temp));
                self.indent += 1;
                self.push(&format!("{} := {}.val", binding, temp));
                for st in &arm.body.stmts {
                    self.emit_stmt(st)?;
                }
                self.indent -= 1;
            } else {
                self.push(&format!("if {}.ok {{", temp));
                self.indent += 1;
                let primary_binding = some_arms[0].0;
                self.push(&format!("{} := {}.val", primary_binding, temp));
                for (i, (binding, arm)) in some_arms.iter().enumerate() {
                    match &arm.guard {
                        Some(g) => {
                            let gt = self.emit_expr(g, 0)?;
                            if i == 0 {
                                self.push(&format!("if {} {{", gt));
                            } else {
                                self.push(&format!("}} else if {} {{", gt));
                            }
                            self.indent += 1;
                            if binding.as_str() != primary_binding.as_str() {
                                self.push(&format!("{} := {}", binding, primary_binding));
                            }
                            for st in &arm.body.stmts {
                                self.emit_stmt(st)?;
                            }
                            self.indent -= 1;
                        }
                        None => {
                            self.push("} else {");
                            self.indent += 1;
                            if binding.as_str() != primary_binding.as_str() {
                                self.push(&format!("{} := {}", binding, primary_binding));
                            }
                            for st in &arm.body.stmts {
                                self.emit_stmt(st)?;
                            }
                            self.indent -= 1;
                            self.push("}");
                            break;
                        }
                    }
                }
                self.indent -= 1;
            }
            match else_arm {
                Some(arm) => {
                    self.push("} else {");
                    self.emit_block_body(&arm.body.stmts)?;
                    self.push("}");
                }
                None => {
                    self.push("}");
                }
            }
        } else {
            let matched_temp = self.next_temp("hyMatched");
            self.push(&format!("{} := false", matched_temp));
            self.push(&format!("if {}.ok {{", temp));
            self.indent += 1;
            let primary_binding = some_arms[0].0;
            self.push(&format!("{} := {}.val", primary_binding, temp));
            for (i, (binding, arm)) in some_arms.iter().enumerate() {
                self.hoist_tries_expr(arm.guard.as_ref().unwrap())?;
                let gt = self.emit_expr(arm.guard.as_ref().unwrap(), 0)?;
                if i == 0 {
                    self.push(&format!("if {} {{", gt));
                } else {
                    self.push(&format!("}} else if {} {{", gt));
                }
                self.indent += 1;
                self.push(&format!("{} = true", matched_temp));
                if binding.as_str() != primary_binding.as_str() {
                    self.push(&format!("{} := {}", binding, primary_binding));
                }
                for st in &arm.body.stmts {
                    self.emit_stmt(st)?;
                }
                self.indent -= 1;
            }
            self.push("}");
            self.indent -= 1;
            self.push("}");

            if let Some(arm) = none_arm {
                if wildcard_arm.is_some() {
                    self.push(&format!("if !{} && !{}.ok {{", matched_temp, temp));
                    self.indent += 1;
                    self.push(&format!("{} = true", matched_temp));
                    for st in &arm.body.stmts {
                        self.emit_stmt(st)?;
                    }
                    self.indent -= 1;
                    self.push("}");
                } else {
                    self.push(&format!("if !{} && !{}.ok {{", matched_temp, temp));
                    self.indent += 1;
                    for st in &arm.body.stmts {
                        self.emit_stmt(st)?;
                    }
                    self.indent -= 1;
                    self.push("}");
                }
            }

            if let Some(arm) = wildcard_arm {
                self.push(&format!("if !{} {{", matched_temp));
                self.indent += 1;
                for st in &arm.body.stmts {
                    self.emit_stmt(st)?;
                }
                self.indent -= 1;
                self.push("}");
            }
        }
        Ok(())
    }
}
