//! Statement emission — `Emitter` impl part 2 (see lib.rs for the context).

use hy_syntax::*;

use super::{to_go_name, BindingKind, EmitError, Emitter};

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
                // a plain goroutine). Route through emit_call so argument
                // variadic spread (`...`), pointer parameter auto-addressing,
                // and package/shape normalization match regular call emission.
                self.hoist_tries_call(callee, args)?;
                let call = self.emit_call(callee, args)?;
                self.push(&format!("go {}", call));
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
                let mut marker = format!(
                    "{{ // managed [{}] {} (audited via hyb check --report-managed)",
                    category, reason
                );
                if !tickets.is_empty() {
                    marker.push_str(&format!(" tickets: {}", tickets.join(", ")));
                }
                self.push(&marker);
                self.emit_block_body(&body.stmts)?;
                self.push("}");
                Ok(())
            }
            Stmt::Defer { call, .. } => {
                // P1 (§7): a deferred CALL's arguments are evaluated at defer
                // time, so a `?` inside them hoists to a binding BEFORE this
                // `defer`, in evaluation order — the same hoist every other
                // statement form runs (define, assign, send, if/for/match
                // conditions). Without it the hoister never registers the
                // temp and `emit_expr` refuses the `?` as "outside a binding
                // position".
                //
                // The operand's parens are peeled for BOTH the hoist gate and
                // the emission, mirroring the checker's HYB-E2001 test, which
                // unwraps parens too (a parenthesized call is still a call).
                // Emission MUST use the peeled operand: Go rejects a
                // parenthesized defer operand ("expression in defer must be
                // function call"), so `defer (f.close())` has to lower to
                // `defer f.Close()` — emitting `defer (f.Close())` would be
                // invalid Go. A non-call operand (which the checker rejects)
                // is left peeled-but-ungated and fails loudly below.
                let operand = Self::peel_parens(call);
                if matches!(operand, Expr::Call { .. }) {
                    self.hoist_tries_expr(operand)?;
                }
                let c = self.emit_expr(operand, 0)?;
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
                        if self.loop_labels.is_empty() {
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
                        if self.loop_labels.is_empty() {
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
                    let saved_scope = self.save_scope();
                    match &arm.kind {
                        SelectArmKind::Recv { names, chan } => {
                            let c = match self.emit_expr(chan, 0) {
                                Ok(c) => c,
                                Err(e) => {
                                    self.restore_scope(saved_scope);
                                    return Err(e);
                                }
                            };
                            let head = match names.len() {
                                0 => format!("case <-{}:", c),
                                1 => {
                                    // An unread or blank single binding cannot emit
                                    // `case _ := <-c:` (Go rejects: `no new variables on
                                    // left side of :=`) or `case v := <-c:` (Go rejects:
                                    // `v declared and not used`). Lower to bare receive.
                                    let read = names[0] != "_"
                                        && crate::rangebind::body_reads(&arm.body, &names[0]);
                                    if read {
                                        format!("case {} := <-{}:", names[0], c)
                                    } else {
                                        format!("case <-{}:", c)
                                    }
                                }
                                2 => {
                                    // Go rejects unread receive bindings (`declared and
                                    // not used`) and rejects `case _, _ := <-c:` (`no new
                                    // variables on left side of :=`).
                                    // If both unread, drop to bare receive. If one is read,
                                    // blank the unread slot with `_`.
                                    let a_read = names[0] != "_"
                                        && crate::rangebind::body_reads(&arm.body, &names[0]);
                                    let b_read = names[1] != "_"
                                        && crate::rangebind::body_reads(&arm.body, &names[1]);
                                    match (a_read, b_read) {
                                        (true, true) => {
                                            format!("case {}, {} := <-{}:", names[0], names[1], c)
                                        }
                                        (true, false) => {
                                            format!("case {}, _ := <-{}:", names[0], c)
                                        }
                                        (false, true) => {
                                            format!("case _, {} := <-{}:", names[1], c)
                                        }
                                        (false, false) => format!("case <-{}:", c),
                                    }
                                }
                                n => {
                                    self.restore_scope(saved_scope);
                                    return Err(EmitError::unsupported(format!(
                                        "select receive with {} names",
                                        n
                                    )));
                                }
                            };
                            self.shadow_local_names(names);
                            self.push(&head);
                        }
                        SelectArmKind::Send { chan, value, .. } => {
                            let c = self.emit_expr(chan, 0)?;
                            let v = self.emit_expr(value, 0)?;
                            self.push(&format!("case {} <- {}:", c, v));
                        }
                        SelectArmKind::Default => self.push("default:"),
                    }
                    let arm_res = self.emit_block_body(&arm.body.stmts);
                    self.restore_scope(saved_scope);
                    arm_res?;
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
        self.emit_local_in_seq(l, &[])
    }

    fn emit_local_in_seq(&mut self, l: &LocalDecl, rest: &[Stmt]) -> Result<(), EmitError> {
        match l.kind {
            LocalKind::Var => {
                let ty = l.ty.as_ref().map(|t| self.type_text(t));
                match (&ty, l.values.first()) {
                    // N5: a zero map (or a struct carrying one) allocates
                    // at its declaration (zeroinit.rs, issue #645).
                    (Some(t), None) => match l.ty.as_ref().and_then(|ty| self.n5_zero_init(ty)) {
                        Some(init) => {
                            let inits = vec![init; l.names.len()].join(", ");
                            self.push(&format!("var {} {} = {}", l.names.join(", "), t, inits))
                        }
                        None => self.push(&format!("var {} {}", l.names.join(", "), t)),
                    },
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
                self.hint_from_values(&l.names, &l.values);
                // Typed var decls carry receiver knowledge (Batch I):
                // `var hits atomic.int64` → shape-known, not a pointer.
                if let Some(t) = &l.ty {
                    if matches!(t, TypeExpr::Pointer { .. }) {
                        for n in &l.names {
                            if n != "_" {
                                self.pointer_vars.insert(n.clone());
                            }
                        }
                    }
                    if let Some(kind) = self.binding_kind_of_ty(t) {
                        for n in &l.names {
                            if n != "_" {
                                self.bindings.insert(n.clone(), kind);
                            }
                        }
                    }
                    if l.names.len() == 1 && l.names[0] != "_" {
                        self.register_var_ty(&l.names[0], t);
                    }
                }
                return Ok(());
            }
            LocalKind::Const => {
                let first = l
                    .values
                    .first()
                    .ok_or_else(|| EmitError::unsupported("`const` without an initializer"))?;
                let value = self.emit_expr(first, 0)?;
                let composite = matches!(
                    Self::peel_parens(first),
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
                self.shadow_local_names(&l.names);
                return Ok(());
            }
            // `:=` and `let` both render as Go's short declaration —
            // immutability is a checker-level contract (D14 folded
            // `let x := e` into this shape).
            LocalKind::Define | LocalKind::Let => {}
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
            let later_has_try = l.values[idx + 1..].iter().any(Self::expr_has_hoistable_try);
            if later_has_try {
                self.hoist_order_before_try(v)?;
            } else {
                self.hoist_tries_expr(v)?;
            }
        }

        if l.ty.is_some() {
            return Err(EmitError::unsupported(
                "type-ascribed `:=` — write `var x T = e` or `x := e`",
            ));
        }

        // `x := &m[k]` / `x := &s.field` (including parallel defines) — the
        // O8 map-slot/field loan. Go cannot take a map-index address; the
        // ownership model's answer (O9: Go-heap reads are copies) lowers to
        // copy-read now + store-back when the loan's non-lexical window ends
        // (or at the declaring block's end). A plain `&x` (local var address)
        // is legal Go and stays verbatim.
        if l.names.len() == l.values.len()
            && l.values
                .iter()
                .any(|v| Self::as_slot_loan_operand(v).is_some())
        {
            let mut val_texts = Vec::with_capacity(l.values.len());
            let mut new_loans = Vec::new();
            for (name, v) in l.names.iter().zip(l.values.iter()) {
                if let Some(operand) = Self::as_slot_loan_operand(v) {
                    let (read_place, write_place) =
                        self.emit_slot_loan_place(operand, name, rest)?;
                    val_texts.push(read_place);
                    if name != "_" {
                        new_loans.push((name.clone(), format!("{} = {}", write_place, name)));
                    }
                } else {
                    val_texts.push(self.emit_expr(v, 0)?);
                }
            }
            self.shadow_local_names(&l.names);
            for (loan_name, _) in &new_loans {
                self.bindings.insert(loan_name.clone(), BindingKind::Plain);
                self.flush_loan_for_name(loan_name);
            }
            self.push(&format!(
                "{} := {}",
                l.names.join(", "),
                val_texts.join(", ")
            ));
            if let Some(frame) = self.loan_frames.last_mut() {
                frame.extend(new_loans);
            }
            return Ok(());
        }

        let values = l
            .values
            .iter()
            .map(|v| self.emit_expr(v, 0))
            .collect::<Result<Vec<_>, _>>()?;
        for name in &l.names {
            if name != "_" {
                self.flush_loan_for_name(name);
            }
        }
        let names = l.names.join(", ");
        self.push(&format!("{} := {}", names, values.join(", ")));
        // A struct-literal define carries the receiver's struct name
        // (Batch I): `stats := pool_stats{}` — field chains type through
        // struct_fields (18's `stats.mu.lock()`).
        self.hint_from_values(&l.names, &l.values);
        Ok(())
    }

    pub(crate) fn expr_key(e: &Expr) -> usize {
        e as *const Expr as usize
    }

    pub(crate) fn peel_parens(mut e: &Expr) -> &Expr {
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
            Expr::StructLit { fields, .. } => fields
                .iter()
                .any(|f| Self::expr_has_hoistable_try(&f.value)),
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

    pub(crate) fn place_has_hoistable_try(p: &Place) -> bool {
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

    fn hoist_tries_call(&mut self, callee: &Expr, args: &[CallArg]) -> Result<(), EmitError> {
        let any_arg_has_try = args.iter().any(|a| Self::expr_has_hoistable_try(&a.value));
        if any_arg_has_try {
            match callee {
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
            Expr::Call { callee, args, .. } => self.hoist_tries_call(callee, args)?,
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
                    if self.is_unshadowed_pkg_root(pkg) {
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
        self.shadow_local_names(&l.names);
        Ok(())
    }

    /// Binding hints from the defining values (see `BindingKind`).
    fn hint_from_values(&mut self, names: &[String], values: &[Expr]) {
        let single_hint = if names.len() == 1 && values.len() == 1 && names[0] != "_" {
            Some(self.binding_kind_of(&values[0]))
        } else {
            None
        };
        let ptr_flags: Vec<bool> = if names.len() == values.len() {
            values.iter().map(|v| self.expr_is_pointer(v)).collect()
        } else {
            Vec::new()
        };
        self.shadow_local_names(names);
        if let Some(kind) = single_hint {
            if kind != BindingKind::Plain {
                self.bindings.insert(names[0].clone(), kind);
            }
            if let Some(Expr::StructLit { ty: Some(t), .. }) = values.first() {
                self.register_var_ty(&names[0], t);
            }
        }
        for (name, is_ptr) in names.iter().zip(ptr_flags) {
            if name != "_" && is_ptr {
                self.pointer_vars.insert(name.clone());
            }
        }
    }

    fn binding_kind_of(&self, e: &Expr) -> BindingKind {
        let e = Self::peel_parens(e);
        if let Expr::StructLit { ty: Some(t), .. } = e {
            if let Some(kind) = self.binding_kind_of_ty(t) {
                return kind;
            }
        }
        let (call, args) = match e {
            Expr::Try { operand, .. } => match operand.as_ref() {
                Expr::Call { callee, args, .. } => (callee.as_ref(), args.as_slice()),
                _ => return BindingKind::Plain,
            },
            Expr::Call { callee, args, .. } => (callee.as_ref(), args.as_slice()),
            _ => return BindingKind::Plain,
        };
        if let Expr::Field { base, name, .. } = call {
            if let Expr::Ident { name: pkg, .. } = base.as_ref() {
                if self.is_unshadowed_pkg_root(pkg) {
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
        // Only classify as Chan if the first argument is a channel type.
        // make([]T, n) (SliceLit) and make(map[K]V) (MapLit) must remain Plain
        // so single-var range binds elements/keys rather than channel values.
        if matches!(call, Expr::Ident { name, .. } if name == "make") {
            if let Some(first) = args.first() {
                if let Expr::StructLit {
                    ty: Some(TypeExpr::Chan { .. }),
                    ..
                } = &first.value
                {
                    return BindingKind::Chan;
                }
            }
            return BindingKind::Plain;
        }
        BindingKind::Plain
    }

    /// Whether an expression evaluates to a channel type (param, typed local,
    /// make(chan ...), or a function returning a channel). Channel ranges in
    /// Go permit only one iteration variable and bind the received value directly.
    fn is_channel_expr(&self, e: &Expr) -> bool {
        match e {
            Expr::Ident { name, .. } => {
                matches!(self.bindings.get(name), Some(BindingKind::Chan))
            }
            Expr::Paren { inner, .. } => self.is_channel_expr(inner),
            Expr::Call { .. } | Expr::Try { .. } => {
                self.binding_kind_of(e) == BindingKind::Chan
            }
            _ => false,
        }
    }

    /// Whether an expression evaluates to a set (`set[T]`). Set ranges in
    /// Go iterate over the underlying map (`s.m`).
    pub(crate) fn is_set_expr(&self, e: &Expr) -> bool {
        let e = Self::peel_parens(e);
        match e {
            Expr::Ident { name, .. } => {
                matches!(self.bindings.get(name), Some(BindingKind::Set))
            }
            Expr::StructLit { ty: Some(t), .. } => match t {
                TypeExpr::Named { name, .. } => name.starts_with("set[") || name == "set",
                _ => false,
            },
            Expr::Field { base, name, .. } => {
                if let Expr::Ident { name: vn, .. } = base.as_ref() {
                    if let Some(sn) = self.var_struct.get(vn) {
                        if let Some(fields) = self.struct_fields.get(sn) {
                            if let Some((_, fty)) = fields.iter().find(|(f, _)| f == name) {
                                return fty.starts_with("set[") || fty == "set";
                            }
                        }
                    }
                }
                false
            }
            Expr::Call { .. } | Expr::Try { .. } => {
                self.binding_kind_of(e) == BindingKind::Set
            }
            _ => false,
        }
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
        self.emit_assign_in_seq(targets, op, values, &[])
    }

    fn emit_assign_in_seq(
        &mut self,
        targets: &[Place],
        op: AssignOp,
        values: &[Expr],
        rest: &[Stmt],
    ) -> Result<(), EmitError> {
        // `x = &m[k]` / `x = &s.field` — the O8 map-slot loan (single or parallel).
        // Go cannot take a map-index address; the ownership model's answer (O9: Go-heap
        // reads are copies) lowers to copy-read now + store-back at the loan's last use
        // (or declaring block's end). Rebinding an existing slot-loan variable flushes
        // its previous store-back before overwriting the variable.
        if op == AssignOp::Assign
            && targets.len() == values.len()
            && !values.is_empty()
            && targets
                .iter()
                .all(|t| matches!(&t.kind, PlaceKind::Ident(_)))
            && values.iter().any(|v| Self::as_slot_loan_operand(v).is_some())
        {
            for (idx, v) in values.iter().enumerate() {
                let later_has_try = values[idx + 1..].iter().any(Self::expr_has_hoistable_try);
                if later_has_try {
                    self.hoist_order_before_try(v)?;
                } else {
                    self.hoist_tries_expr(v)?;
                }
            }
            let names: Vec<String> = targets
                .iter()
                .map(|t| match &t.kind {
                    PlaceKind::Ident(n) => n.clone(),
                    _ => unreachable!(),
                })
                .collect();
            for name in &names {
                if name != "_" {
                    self.flush_loan_for_name(name);
                }
            }
            let mut rhs_texts = Vec::with_capacity(values.len());
            let mut new_loans = Vec::new();
            for (name, v) in names.iter().zip(values.iter()) {
                if let Some(operand) = Self::as_slot_loan_operand(v) {
                    let (read_place, write_place) =
                        self.emit_slot_loan_place(operand, name, rest)?;
                    if self.pointer_vars.contains(name) {
                        // A pointer-typed target (`var x *T`, a `*T` param,
                        // `x := &T{}`) keeps its Go type: point it at a copy
                        // of the slot and store the pointee back.
                        let copy = self.next_temp("hyLoan");
                        self.push(&format!("{} := {}", copy, read_place));
                        rhs_texts.push(format!("&{}", copy));
                        new_loans.push((name.clone(), format!("{} = *{}", write_place, name)));
                    } else {
                        rhs_texts.push(read_place);
                        if name != "_" {
                            new_loans.push((name.clone(), format!("{} = {}", write_place, name)));
                        }
                    }
                } else {
                    rhs_texts.push(self.emit_expr(v, 0)?);
                }
            }
            self.push(&format!("{} = {}", names.join(", "), rhs_texts.join(", ")));
            if let Some(frame) = self.loan_frames.last_mut() {
                frame.extend(new_loans);
            }
            return Ok(());
        }
        if targets.len() == values.len() && op == AssignOp::Assign {
            let ptr_flags: Vec<bool> = values.iter().map(|v| self.expr_is_pointer(v)).collect();
            for (t, is_ptr) in targets.iter().zip(ptr_flags) {
                if let PlaceKind::Ident(name) = &t.kind {
                    if name != "_" {
                        if is_ptr {
                            self.pointer_vars.insert(name.clone());
                        } else {
                            self.pointer_vars.remove(name);
                        }
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
        if self.is_unshadowed_pkg_root(root)
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
        // `(x).f` resolves as `x.f`: a redundant receiver paren must not hide
        // a package root or a foreign binding from the spelling decision.
        if let Expr::Ident { name: root, .. } = Self::peel_parens(base) {
            if self.is_unshadowed_pkg_root(root) {
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
        let saved_scope = self.save_scope();
        let res = self.emit_if_inner(init, cond, then, otherwise);
        self.restore_scope(saved_scope);
        res
    }

    fn emit_if_inner(
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
                for (_, store) in frame {
                    self.push(&store);
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
        let saved_scope = self.save_scope();
        let res = self.emit_if_at_inner(init, cond, then, otherwise, at_line_start);
        self.restore_scope(saved_scope);
        res
    }

    fn emit_if_at_inner(
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
            None => {
                // #546: with no `else`, the no-branch-taken path still owes
                // the store-back of any loan handed down into this `if`.
                if let Some(inherited) = self.branch_inherit.clone() {
                    self.push("} else {");
                    self.indent += 1;
                    for (_, store) in inherited {
                        self.push(&store);
                    }
                    self.indent -= 1;
                }
                self.push("}")
            }
            Some(
                else_if @ Stmt::If {
                    init,
                    cond,
                    then,
                    otherwise,
                    ..
                },
            ) => {
                if self.branch_inherit.is_some() {
                    self.push("} else {");
                    self.emit_block_body(std::slice::from_ref(else_if))?;
                    self.push("}");
                } else if init
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
                self.hint_from_values(&l.names, &l.values);
                if let Some(t) = &l.ty {
                    if matches!(t, TypeExpr::Pointer { .. }) {
                        for n in &l.names {
                            if n != "_" {
                                self.pointer_vars.insert(n.clone());
                            }
                        }
                    }
                    if let Some(kind) = self.binding_kind_of_ty(t) {
                        for n in &l.names {
                            if n != "_" {
                                self.bindings.insert(n.clone(), kind);
                            }
                        }
                    }
                    if l.names.len() == 1 && l.names[0] != "_" {
                        self.register_var_ty(&l.names[0], t);
                    }
                }
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
    /// statements render (flushing any loan whose non-lexical `last_use`
    /// ended at a statement), and any remaining store-backs flush at the
    /// block's end (before the closing brace) unless the block already
    /// terminated via `return`, `break`, or `continue` (#218/#203 flush
    /// active loans before the control-flow transfer). This is what makes
    /// the O8 slot-loan desugar sound: mutations through the loaned copy
    /// persist exactly for the loan's non-lexical live window.
    pub fn emit_block_body(&mut self, stmts: &[Stmt]) -> Result<(), EmitError> {
        // #546: a branch of a hand-off statement starts with the inherited
        // loans; `take` keeps nested blocks inside the branch from inheriting
        // them again, and the restore hands the same set to sibling branches.
        let inherited = self.branch_inherit.take();
        self.loan_frames.push(inherited.clone().unwrap_or_default());
        self.indent += 1;
        let r = self.emit_stmts_with_loans(stmts);
        self.indent -= 1;
        self.branch_inherit = inherited;
        r
    }

    fn emit_arm_stmts_with_loans(&mut self, stmts: &[Stmt]) -> Result<(), EmitError> {
        let inherited = self.branch_inherit.take();
        self.loan_frames.push(inherited.clone().unwrap_or_default());
        let r = self.emit_stmts_with_loans(stmts);
        self.branch_inherit = inherited;
        r
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
        if let Some(inherited) = &self.branch_inherit {
            for entry in inherited {
                if !entry.0.is_empty() {
                    stores.push(entry.clone());
                }
            }
        }
        for frame in self.loan_frames.iter().rev() {
            if frame.first().map(|(n, _)| n.is_empty()).unwrap_or(false) {
                continue;
            }
            for entry in frame {
                stores.push(entry.clone());
            }
        }
        for (_, store) in stores {
            self.push(&store);
        }
    }

    fn flush_loans_for_loop(&mut self, labeled: bool) {
        let mut stores: Vec<(String, String)> = Vec::new();
        if let Some(inherited) = &self.branch_inherit {
            for entry in inherited {
                if !entry.0.is_empty() {
                    stores.push(entry.clone());
                }
            }
        }
        for frame in self.loan_frames.iter().rev() {
            if frame.first().map(|(n, _)| n.is_empty()).unwrap_or(false) {
                if !labeled || !frame[0].1.is_empty() {
                    break;
                }
                continue;
            }
            for entry in frame {
                stores.push(entry.clone());
            }
        }
        for (_, store) in stores {
            self.push(&store);
        }
    }

    fn emit_for(&mut self, kind: &ForKind, body: &Block) -> Result<(), EmitError> {
        let saved_scope = self.save_scope();
        let res = self.emit_for_inner(kind, body);
        self.restore_scope(saved_scope);
        res
    }

    fn emit_for_inner(&mut self, kind: &ForKind, body: &Block) -> Result<(), EmitError> {
        // A labeled `break loop` / `continue loop` inside this loop (NOT
        // inside an inner loop statement — that one labels itself) makes this loop the
        // Go label target: `hyLoop:` + the for head, `break hyLoop` inside.
        // SPEC LX14: `break loop` / `continue loop` binds ONLY to the nearest enclosing
        // `loop` statement (`ForKind::Infinite`), NEVER to a `for` (range/iterate/while).
        let is_infinite_loop = matches!(kind, ForKind::Infinite);
        let needs_label = is_infinite_loop && self.block_has_loop_break(body);
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
                self.indent += 1;
                self.hoist_tries_expr(cond)?;
                let c = self.emit_expr(cond, ATOM)?;
                self.push(&format!("if !{} {{", c));
                self.indent += 1;
                self.push("break");
                self.indent -= 1;
                self.push("}");
                self.emit_arm_stmts_with_loans(&body.stmts)?;
                self.indent -= 1;
                self.push("}");
                if label.is_some() {
                    self.loop_labels.pop();
                }
                return Ok(());
            }
        }
        // When set, the inclusive-range arm asks us to inject
        // `cont = v != hi` as the first body statement (see #322).
        let mut inclusive_stop: Option<(String, String, String)> = None;
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
                // `for _ in a..b` (differential corpus 16/17/18): Go has no
                // blank := variable — a fresh temp carries the count. The
                // semantics are identical: the variable is never read.
                let v = if var == "_" {
                    self.next_temp("hyBlank")
                } else {
                    self.shadow_local_name(var);
                    var.clone()
                };
                if *inclusive {
                    // Issue #322: `for v := lo; v <= hi; v++` wraps past the
                    // type maximum (int8 127..=127 never stops). Emit a
                    // continue-safe form that still runs the body once at
                    // `hi` and exits even when `continue` hits the post:
                    //   hyLo := lo; hyHi := hi
                    //   for v, cont := hyLo, hyLo <= hyHi; cont; v++ {
                    //       cont = v != hyHi
                    //       body
                    //   }
                    let lo_once = self.next_temp("hyLo");
                    let hi_once = self.next_temp("hyHi");
                    let cont = self.next_temp("hyCont");
                    self.push(&format!("{} := {}", lo_once, lo_t));
                    self.push(&format!("{} := {}", hi_once, hi_t));
                    inclusive_stop = Some((cont.clone(), v.clone(), hi_once.clone()));
                    format!(
                        "for {}, {} := {}, {} <= {}; {}; {}++ {{",
                        v, cont, lo_once, lo_once, hi_once, cont, v
                    )
                } else {
                    format!("for {} := {}; {} < {}; {}++ {{", v, lo_t, v, hi_t, v)
                }
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
                        // iterable. When a named loop variable is never read in
                        // the body, Go rejects it (`declared and not used`).
                        // Drop the binding to `for range e`, completing the
                        // unread-binding lowering from issue #139.
                        let read = vars[0] != "_" && crate::rangebind::body_reads(body, &vars[0]);
                        if self.is_set_expr(expr) {
                            self.shadow_local_names(vars);
                            if !read {
                                format!("for range {}.m {{", e)
                            } else {
                                format!("for {} := range {}.m {{", vars[0], e)
                            }
                        } else if !read {
                            self.shadow_local_names(vars);
                            format!("for range {} {{", e)
                        } else {
                            // Hybrid's single-var iterate binds the VALUE.
                            // Go's single-var range binds the index (slices/
                            // maps) or the value (channels) — hence the Chan
                            // hint check.
                            let over_chan = self.is_channel_expr(expr);
                            if over_chan {
                                self.shadow_local_names(vars);
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
                                let elem_ty = range_name.and_then(|n| match self.bindings.get(&n) {
                                    Some(BindingKind::ForeignTy(t)) => Some(*t),
                                    _ => None,
                                });
                                self.shadow_local_names(vars);
                                if let Some(t) = elem_ty {
                                    self.bindings
                                        .insert(vars[0].clone(), BindingKind::ForeignTy(t));
                                }
                                format!("for _, {} := range {} {{", vars[0], e)
                            }
                        }
                    }
                    2 => {
                        if self.is_channel_expr(expr) {
                            return Err(EmitError::unsupported(
                                "cannot range over channel with two iteration variables: channels permit only one iteration variable (SPEC-LANGUAGE.md LX13)",
                            ));
                        }
                        if self.is_set_expr(expr) {
                            return Err(EmitError::unsupported(
                                "cannot range over set with two iteration variables: sets permit only one iteration variable (SPEC-LANGUAGE.md T9)",
                            ));
                        }
                        self.shadow_local_names(vars);
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
        // Inclusive-range stop-flag (#322): `hyCont = v != hyHi` runs as the
        // FIRST body statement, so `continue` still evaluates it on the last
        // value and the loop exits instead of wrapping past the type maximum.
        // The body itself goes through emit_block_body so terminator flushes
        // and the loan-frame sentinel machinery apply unchanged.
        if let Some((cont, v, hi_once)) = &inclusive_stop {
            self.indent += 1;
            self.push(&format!("{} = {} != {}", cont, v, hi_once));
            self.indent -= 1;
        }
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
            Stmt::For { kind, body, .. } => {
                if matches!(kind, ForKind::Infinite) {
                    false // inner infinite loop owns breaks directly inside it
                } else {
                    self.block_has_loop_break(body)
                }
            }
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
                    Some(zeros) if i < zeros.len() => {
                        let z = &zeros[i];
                        vs.push(if z.is_empty() {
                            "nil".to_string()
                        } else {
                            z.clone()
                        });
                    }
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
        if vs.is_empty() {
            self.push("return");
        } else {
            self.push(&format!("return {}", vs.join(", ")));
        }
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

        self.hoist_tries_expr(subject)?;
        let temp = self.next_temp("hyMatch");
        let s = self.emit_expr(subject, 0)?;
        self.push(&format!("{} := {}", temp, s));

        let mut some_arms: Vec<(&String, &MatchArm)> = Vec::new();
        let mut none_arm: Option<&MatchArm> = None;
        let mut wildcard_arm: Option<&MatchArm> = None;
        for (kind, arm) in &classified {
            match kind {
                ArmKind::Some(binding) => {
                    some_arms.push((binding, *arm));
                }
                ArmKind::NoneArm => none_arm = Some(*arm),
                ArmKind::Wildcard => wildcard_arm = Some(*arm),
            }
        }

        if some_arms.is_empty() {
            // No some-arm: the checker's exhaustiveness (E5001) rejects
            // this before emission; nothing to lower.
            return Ok(());
        }

        let reachable_len = some_arms
            .iter()
            .position(|(_, arm)| arm.guard.is_none())
            .map(|idx| idx + 1)
            .unwrap_or(some_arms.len());
        let reachable_some_arms = &some_arms[..reachable_len];
        let has_unguarded_some = reachable_some_arms
            .iter()
            .any(|(_, arm)| arm.guard.is_none());
        let simple_unguarded_chain = has_unguarded_some
            && match reachable_some_arms {
                [(_, arm0)] => arm0.guard.is_none(),
                [(b0, arm0), (b1, arm1)] => {
                    arm0.guard.is_some()
                        && arm1.guard.is_none()
                        && !(*b0 != "_"
                            && arm0
                                .guard
                                .as_ref()
                                .is_some_and(|g| crate::rangebind::expr_reads(g, b0))
                            && *b1 != *b0
                            && crate::rangebind::body_reads(&arm1.body, b0))
                }
                _ => false,
            };

        if simple_unguarded_chain {
            let else_arm = none_arm.or(wildcard_arm);
            self.push(&format!("if {}.ok {{", temp));
            self.indent += 1;
            let mut prebound_guard_var: Option<String> = None;
            if let Some((b0, arm0)) = reachable_some_arms.first() {
                if **b0 != "_"
                    && arm0
                        .guard
                        .as_ref()
                        .is_some_and(|g| crate::rangebind::expr_reads(g, b0))
                {
                    self.push(&format!("{} := {}.val", b0, temp));
                    prebound_guard_var = Some((*b0).clone());
                }
            }
            let mut chain_opened = false;
            for (binding, arm) in reachable_some_arms.iter() {
                let saved_scope = self.save_scope();
                self.shadow_local_name(binding);
                let arm_res = (|| -> Result<bool, EmitError> {
                    match &arm.guard {
                        Some(g) => {
                            self.hoist_tries_expr(g)?;
                            let gt = self.emit_expr(g, 0)?;
                            self.push(&format!("if {} {{", gt));
                            self.indent += 1;
                            if *binding != "_"
                                && prebound_guard_var.as_deref() != Some(binding.as_str())
                                && crate::rangebind::body_reads(&arm.body, binding)
                            {
                                self.push(&format!("{} := {}.val", binding, temp));
                            }
                            self.emit_arm_stmts_with_loans(&arm.body.stmts)?;
                            self.indent -= 1;
                            chain_opened = true;
                            Ok(false)
                        }
                        None => {
                            if chain_opened {
                                self.push("} else {");
                                self.indent += 1;
                            }
                            if *binding != "_"
                                && prebound_guard_var.as_deref() != Some(binding.as_str())
                                && crate::rangebind::body_reads(&arm.body, binding)
                            {
                                self.push(&format!("{} := {}.val", binding, temp));
                            }
                            self.emit_arm_stmts_with_loans(&arm.body.stmts)?;
                            if chain_opened {
                                self.indent -= 1;
                                self.push("}");
                            }
                            Ok(true)
                        }
                    }
                })();
                self.restore_scope(saved_scope);
                if arm_res? {
                    break;
                }
            }
            self.indent -= 1;
            match else_arm {
                Some(arm) => {
                    self.push("} else {");
                    self.emit_block_body(&arm.body.stmts)?;
                    self.push("}");
                }
                None => {
                    if let Some(inherited) = self.branch_inherit.clone() {
                        self.push("} else {");
                        self.indent += 1;
                        for (_, store) in inherited {
                            self.push(&store);
                        }
                        self.indent -= 1;
                    }
                    self.push("}");
                }
            }
        } else if has_unguarded_some {
            let else_arm = none_arm.or(wildcard_arm);
            self.push(&format!("if {}.ok {{", temp));
            self.indent += 1;
            let matched_temp = self.next_temp("hyMatched");
            self.push(&format!("{} := false", matched_temp));
            for (i, (binding, arm)) in reachable_some_arms.iter().enumerate() {
                if i == 0 {
                    self.push("{");
                } else {
                    self.push(&format!("if !{} {{", matched_temp));
                }
                self.indent += 1;
                let saved_scope = self.save_scope();
                self.shadow_local_name(binding);
                let arm_res = (|| -> Result<(), EmitError> {
                    match &arm.guard {
                        Some(g) => {
                            let guard_reads =
                                *binding != "_" && crate::rangebind::expr_reads(g, binding);
                            if guard_reads {
                                self.push(&format!("{} := {}.val", binding, temp));
                            }
                            self.hoist_tries_expr(g)?;
                            let gt = self.emit_expr(g, 0)?;
                            self.push(&format!("if {} {{", gt));
                            self.indent += 1;
                            self.push(&format!("{} = true", matched_temp));
                            if *binding != "_"
                                && !guard_reads
                                && crate::rangebind::body_reads(&arm.body, binding)
                            {
                                self.push(&format!("{} := {}.val", binding, temp));
                            }
                            self.emit_arm_stmts_with_loans(&arm.body.stmts)?;
                            self.indent -= 1;
                            self.push("}");
                        }
                        None => {
                            if *binding != "_" && crate::rangebind::body_reads(&arm.body, binding) {
                                self.push(&format!("{} := {}.val", binding, temp));
                            }
                            self.emit_arm_stmts_with_loans(&arm.body.stmts)?;
                        }
                    }
                    Ok(())
                })();
                self.restore_scope(saved_scope);
                self.indent -= 1;
                self.push("}");
                arm_res?;
            }
            self.indent -= 1;
            match else_arm {
                Some(arm) => {
                    self.push("} else {");
                    self.emit_block_body(&arm.body.stmts)?;
                    self.push("}");
                }
                None => {
                    if let Some(inherited) = self.branch_inherit.clone() {
                        self.push("} else {");
                        self.indent += 1;
                        for (_, store) in inherited {
                            self.push(&store);
                        }
                        self.indent -= 1;
                    }
                    self.push("}");
                }
            }
        } else {
            let matched_temp = self.next_temp("hyMatched");
            self.push(&format!("{} := false", matched_temp));
            self.push(&format!("if {}.ok {{", temp));
            self.indent += 1;
            let multi_arms = reachable_some_arms.len() > 1;
            for (i, (binding, arm)) in reachable_some_arms.iter().enumerate() {
                if multi_arms {
                    if i == 0 {
                        self.push("{");
                    } else {
                        self.push(&format!("if !{} {{", matched_temp));
                    }
                    self.indent += 1;
                }
                let saved_scope = self.save_scope();
                self.shadow_local_name(binding);
                let arm_res = (|| -> Result<(), EmitError> {
                    let g = arm.guard.as_ref().unwrap();
                    let guard_reads =
                        *binding != "_" && crate::rangebind::expr_reads(g, binding);
                    if guard_reads {
                        self.push(&format!("{} := {}.val", binding, temp));
                    }
                    self.hoist_tries_expr(g)?;
                    let gt = self.emit_expr(g, 0)?;
                    self.push(&format!("if {} {{", gt));
                    self.indent += 1;
                    self.push(&format!("{} = true", matched_temp));
                    if *binding != "_"
                        && !guard_reads
                        && crate::rangebind::body_reads(&arm.body, binding)
                    {
                        self.push(&format!("{} := {}.val", binding, temp));
                    }
                    self.emit_arm_stmts_with_loans(&arm.body.stmts)?;
                    self.indent -= 1;
                    self.push("}");
                    Ok(())
                })();
                self.restore_scope(saved_scope);
                if multi_arms {
                    self.indent -= 1;
                    self.push("}");
                }
                arm_res?;
            }
            self.indent -= 1;
            self.push("}");

            if let Some(arm) = none_arm {
                if wildcard_arm.is_some() || self.branch_inherit.is_some() {
                    self.push(&format!("if !{} && !{}.ok {{", matched_temp, temp));
                    self.indent += 1;
                    self.push(&format!("{} = true", matched_temp));
                    self.emit_arm_stmts_with_loans(&arm.body.stmts)?;
                    self.indent -= 1;
                    self.push("}");
                } else {
                    self.push(&format!("if !{} && !{}.ok {{", matched_temp, temp));
                    self.indent += 1;
                    self.emit_arm_stmts_with_loans(&arm.body.stmts)?;
                    self.indent -= 1;
                    self.push("}");
                }
            }

            if let Some(arm) = wildcard_arm {
                self.push(&format!("if !{} {{", matched_temp));
                self.indent += 1;
                self.emit_arm_stmts_with_loans(&arm.body.stmts)?;
                self.indent -= 1;
                self.push("}");
            } else if let Some(inherited) = self.branch_inherit.clone() {
                self.push(&format!("if !{} {{", matched_temp));
                self.indent += 1;
                for (_, store) in inherited {
                    self.push(&store);
                }
                self.indent -= 1;
                self.push("}");
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // O8 slot-loan helpers (non-lexical last_use flush, rebind flush, key snapshot)
    // -----------------------------------------------------------------------

    /// Returns `Some(operand)` when `expr` is an O8 slot-loan `&m[k]` or `&s.field`
    /// (including parenthesized forms `(&m[k])` and `&(m[k])`).
    fn as_slot_loan_operand(expr: &Expr) -> Option<&Expr> {
        match Self::peel_parens(expr) {
            Expr::AddrOf { operand, .. }
            | Expr::Unary {
                op: UnOp::Addr,
                operand,
                ..
            } => {
                let inner = Self::peel_parens(operand);
                if matches!(inner, Expr::Index { .. } | Expr::Field { .. }) {
                    Some(inner)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Emits the read-place and store-back-place for an O8 slot loan `&operand`
    /// bound to `loan_var`, snapshotting the index expression into a fresh
    /// `hyKeyN` temporary when the index is non-trivial or is mutated before
    /// the loan's store-back.
    fn emit_slot_loan_place(
        &mut self,
        operand: &Expr,
        loan_var: &str,
        rest: &[Stmt],
    ) -> Result<(String, String), EmitError> {
        if let Expr::Index { base, index, .. } = operand {
            let needs_key_snapshot = match index.as_ref() {
                Expr::Lit { .. } => false,
                Expr::Ident { name, .. } => {
                    name == loan_var
                        || rest.iter().any(|s| Self::stmt_modifies_ident(s, name))
                }
                _ => true,
            };
            let base_mentions_loan_var =
                loan_var != "_" && Self::expr_mentions_ident(base, loan_var);
            if needs_key_snapshot || base_mentions_loan_var {
                let base_text = self.emit_expr(base, ATOM)?;
                let idx_text = self.emit_expr(index, 0)?;
                let key_place = if needs_key_snapshot {
                    let kt = self.next_temp("hyKey");
                    self.push(&format!("{} := {}", kt, idx_text));
                    kt
                } else {
                    idx_text
                };
                let place = format!("{}[{}]", base_text, key_place);
                return Ok((place.clone(), place));
            }
        }
        let place = self.emit_expr(operand, 0)?;
        Ok((place.clone(), place))
    }

    /// Flushes and removes any active slot-loan entries for `target_name` in
    /// the current loan frame (used before rebinding `target_name = &...`).
    fn flush_loan_for_name(&mut self, target_name: &str) {
        let mut to_flush = Vec::new();
        if let Some(frame) = self.loan_frames.last_mut() {
            let mut i = 0;
            while i < frame.len() {
                if frame[i].0 == target_name {
                    to_flush.push(frame.remove(i));
                } else {
                    i += 1;
                }
            }
        }
        for (_, store) in to_flush {
            self.push(&store);
        }
    }

    /// Emits a sequence of statements inside the current loan frame, flushing
    /// each active slot loan as soon as the remaining statements in the block
    /// no longer reference the loan variable (matching `hy-check`'s non-lexical
    /// `last_use` window), and flushing any still-active loans at block exit.
    fn emit_stmts_with_loans(&mut self, stmts: &[Stmt]) -> Result<(), EmitError> {
        let saved_scope = self.save_scope();
        let res = self.emit_stmts_with_loans_inner(stmts);
        self.restore_scope(saved_scope);
        res
    }

    fn emit_stmts_with_loans_inner(&mut self, stmts: &[Stmt]) -> Result<(), EmitError> {
        // #546: a loan inherited from an enclosing hand-off statement that this
        // branch never uses is stored back before the branch's first
        // statement, i.e. before any spawn/send/delete that touches its base.
        let mut unused = Vec::new();
        if let Some(frame) = self.loan_frames.last_mut() {
            let mut i = 0;
            while i < frame.len() {
                if !frame[i].0.is_empty() && !Self::remaining_stmts_use_loan(stmts, &frame[i].0) {
                    unused.push(frame.remove(i));
                } else {
                    i += 1;
                }
            }
        }
        for (_, store) in unused {
            self.push(&store);
        }
        for (idx, st) in stmts.iter().enumerate() {
            let rest = &stmts[idx + 1..];
            // #546: a compound statement (`if`, `match`, `select`, `block`,
            // `managed`, `for`) that is the loan's last user AND spawns, sends,
            // deletes, clears, or writes using the loan's base map must not get
            // a single store-back after it: on a path that moved or mutated the
            // map, that write races the new task or clobbers the branch's write.
            // Hand the loan down into every branch instead (or pre-flush when
            // the statement's own header shadows the loan name or sends the
            // base in a select case clause).
            let mut pre_flush = Vec::new();
            let mut handoff = Vec::new();
            if Self::stmt_supports_loan_handoff(st) {
                if let Some(frame) = self.loan_frames.last_mut() {
                    let mut i = 0;
                    while i < frame.len() {
                        let (name, place) = &frame[i];
                        let root = Self::place_root(place);
                        if !name.is_empty()
                            && !Self::remaining_stmts_use_loan(rest, name)
                            && Self::stmt_has_boundary_using(st, name, root)
                        {
                            let must_pre_flush = Self::stmt_must_pre_flush_loan(st, name, root);
                            let entry = frame.remove(i);
                            if must_pre_flush {
                                pre_flush.push(entry);
                            } else {
                                if matches!(st, Stmt::For { .. }) {
                                    pre_flush.push(entry.clone());
                                }
                                handoff.push(entry);
                            }
                        } else {
                            i += 1;
                        }
                    }
                }
            }
            for (_, store) in pre_flush {
                self.push(&store);
            }
            let saved_inherit = if handoff.is_empty() {
                None
            } else {
                Some(self.branch_inherit.replace(handoff))
            };
            match st {
                Stmt::Local(l) => self.emit_local_in_seq(l, rest)?,
                Stmt::Assign {
                    targets,
                    op,
                    values,
                    ..
                } => self.emit_assign_in_seq(targets, *op, values, rest)?,
                _ => self.emit_stmt(st)?,
            }
            if let Some(prev) = saved_inherit {
                self.branch_inherit = prev;
            }
            // If `rest` can still execute after `st`, flush any loan in the
            // current frame whose variable has no remaining uses in `rest`
            // (or is shadowed/rebound before any read of the current loan).
            if !rest.is_empty() && !Self::stmt_terminates(st) {
                let mut early_flush = Vec::new();
                if let Some(frame) = self.loan_frames.last_mut() {
                    let mut i = 0;
                    while i < frame.len() {
                        if !Self::remaining_stmts_use_loan(rest, &frame[i].0) {
                            early_flush.push(frame.remove(i));
                        } else {
                            i += 1;
                        }
                    }
                }
                for (name, store) in early_flush {
                    if !name.is_empty() {
                        self.push(&store);
                    }
                }
            }
        }
        let frame = self.loan_frames.pop().unwrap_or_default();
        if !stmts.iter().any(Self::stmt_terminates) {
            for (name, store) in frame {
                if !name.is_empty() {
                    self.push(&store);
                }
            }
        }
        Ok(())
    }

    /// Leading identifier of a rendered loan place (`m["a"]` -> `m`, `h.m[k]` -> `h`).
    fn place_root(place: &str) -> &str {
        let end = place
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(place.len());
        &place[..end]
    }

    fn stmt_supports_loan_handoff(st: &Stmt) -> bool {
        matches!(
            st,
            Stmt::If { .. }
                | Stmt::Match { .. }
                | Stmt::Select { .. }
                | Stmt::Block { .. }
                | Stmt::Managed { .. }
                | Stmt::For { .. }
        )
    }

    fn stmt_must_pre_flush_loan(st: &Stmt, name: &str, root: &str) -> bool {
        match st {
            Stmt::If { init: Some(i), .. } => Self::local_declares(i, name),
            Stmt::Match { arms, .. } => arms.iter().any(|a| {
                matches!(
                    &a.pattern,
                    MatchPattern::Path {
                        binding: Some(b),
                        ..
                    } if b == name
                )
            }),
            Stmt::Select { arms, .. } => arms.iter().any(|a| {
                Self::select_arm_binds(&a.kind, name)
                    || matches!(
                        &a.kind,
                        SelectArmKind::Send { value, .. }
                            if Self::expr_mentions_ident(value, root)
                                || Self::expr_has_boundary_using(value, root)
                    )
            }),
            Stmt::For { kind, body, .. } => {
                Self::for_binder_shadows(kind, name) || !Self::block_mentions_ident(body, name)
            }
            _ => false,
        }
    }

    /// #546 / #830: does `st` (recursively) contain a task spawn, channel send,
    /// select send, `delete`/`clear`, direct write/move touching `root` (or an
    /// alias), or a branch block that uses `name` (in its header or statements)
    /// and subsequently reads `root`?
    fn stmt_has_boundary_using(st: &Stmt, name: &str, root: &str) -> bool {
        Self::stmt_has_boundary_using_with_seen(st, name, root, false)
    }

    fn stmt_has_boundary_using_with_seen(
        st: &Stmt,
        name: &str,
        root: &str,
        seen_loan_use: bool,
    ) -> bool {
        if root.is_empty() {
            return false;
        }
        let in_block = |b: &Block, mut seen: bool| {
            for s in &b.stmts {
                if seen && Self::stmt_mentions_ident(s, root) {
                    return true;
                }
                if Self::stmt_has_boundary_using_with_seen(s, name, root, seen) {
                    return true;
                }
                if Self::stmt_mentions_ident(s, name) {
                    seen = true;
                }
            }
            false
        };
        match st {
            Stmt::Task { callee, args, .. } => {
                Self::expr_mentions_ident(callee, root)
                    || Self::expr_has_boundary_using(callee, root)
                    || args.iter().any(|a| {
                        Self::expr_mentions_ident(&a.value, root)
                            || Self::expr_has_boundary_using(&a.value, root)
                    })
            }
            Stmt::Send { chan, value, .. } => {
                Self::expr_mentions_ident(chan, root)
                    || Self::expr_mentions_ident(value, root)
                    || Self::expr_has_boundary_using(chan, root)
                    || Self::expr_has_boundary_using(value, root)
            }
            Stmt::Defer { call, .. } => {
                Self::expr_mentions_ident(call, root) || Self::expr_has_boundary_using(call, root)
            }
            Stmt::Assign {
                targets, values, ..
            } => {
                targets
                    .iter()
                    .any(|t| t.root() == root || matches!(t.kind, PlaceKind::Index { .. }))
                    || values.iter().any(|v| Self::expr_has_boundary_using(v, root))
            }
            Stmt::Local(l) => l
                .values
                .iter()
                .any(|v| Self::expr_has_boundary_using(v, root)),
            Stmt::Return { values, .. } => values
                .iter()
                .any(|v| Self::expr_has_boundary_using(v, root)),
            Stmt::Expr { expr, .. } => Self::expr_has_boundary_using(expr, root),
            Stmt::If {
                init,
                cond,
                then,
                otherwise,
                ..
            } => {
                let init_has = init.as_deref().is_some_and(|i| {
                    (seen_loan_use && Self::stmt_mentions_ident(i, root))
                        || Self::stmt_has_boundary_using_with_seen(i, name, root, seen_loan_use)
                });
                let seen_after_init = seen_loan_use
                    || init
                        .as_deref()
                        .is_some_and(|i| Self::stmt_mentions_ident(i, name));
                let cond_has = (seen_after_init && Self::expr_mentions_ident(cond, root))
                    || Self::expr_has_boundary_using(cond, root);
                let shadows = init
                    .as_deref()
                    .is_some_and(|i| Self::local_declares(i, name));
                let seen_after_header =
                    seen_after_init || (!shadows && Self::expr_mentions_ident(cond, name));
                init_has
                    || cond_has
                    || in_block(then, seen_after_header)
                    || otherwise.as_deref().is_some_and(|o| {
                        Self::stmt_has_boundary_using_with_seen(o, name, root, seen_after_header)
                    })
            }
            Stmt::For { kind, body, .. } => {
                let kind_mentions_root = match kind {
                    ForKind::Infinite => false,
                    ForKind::While { cond } => Self::expr_mentions_ident(cond, root),
                    ForKind::Range { lo, hi, .. } => {
                        Self::expr_mentions_ident(lo, root) || Self::expr_mentions_ident(hi, root)
                    }
                    ForKind::Iterate { expr, .. } => Self::expr_mentions_ident(expr, root),
                };
                let kind_mentions_name = match kind {
                    ForKind::Infinite => false,
                    ForKind::While { cond } => Self::expr_mentions_ident(cond, name),
                    ForKind::Range { lo, hi, .. } => {
                        Self::expr_mentions_ident(lo, name) || Self::expr_mentions_ident(hi, name)
                    }
                    ForKind::Iterate { expr, .. } => Self::expr_mentions_ident(expr, name),
                };
                let kind_has_boundary = match kind {
                    ForKind::Infinite => false,
                    ForKind::While { cond } => Self::expr_has_boundary_using(cond, root),
                    ForKind::Range { lo, hi, .. } => {
                        Self::expr_has_boundary_using(lo, root)
                            || Self::expr_has_boundary_using(hi, root)
                    }
                    ForKind::Iterate { expr, .. } => Self::expr_has_boundary_using(expr, root),
                };
                let seen_after_header = seen_loan_use || kind_mentions_name;
                (seen_loan_use && kind_mentions_root)
                    || kind_has_boundary
                    || in_block(body, seen_after_header)
            }
            Stmt::Block { block, .. } => in_block(block, seen_loan_use),
            Stmt::Managed { body, .. } => in_block(body, seen_loan_use),
            Stmt::Match { subject, arms, .. } => {
                let subj_has = (seen_loan_use && Self::expr_mentions_ident(subject, root))
                    || Self::expr_has_boundary_using(subject, root);
                let seen_after_subj = seen_loan_use || Self::expr_mentions_ident(subject, name);
                subj_has
                    || arms.iter().any(|a| {
                        let shadowed = matches!(
                            &a.pattern,
                            MatchPattern::Path {
                                binding: Some(b),
                                ..
                            } if b == name
                        );
                        let guard_has = a.guard.as_ref().is_some_and(|g| {
                            (seen_after_subj && Self::expr_mentions_ident(g, root))
                                || Self::expr_has_boundary_using(g, root)
                        });
                        let seen_after_guard = seen_after_subj
                            || (!shadowed
                                && a.guard
                                    .as_ref()
                                    .is_some_and(|g| Self::expr_mentions_ident(g, name)));
                        guard_has || in_block(&a.body, seen_after_guard)
                    })
            }
            Stmt::Select { arms, .. } => arms.iter().any(|a| {
                let head_has = match &a.kind {
                    SelectArmKind::Recv { chan, .. } => {
                        (seen_loan_use && Self::expr_mentions_ident(chan, root))
                            || Self::expr_has_boundary_using(chan, root)
                    }
                    SelectArmKind::Send { chan, value, .. } => {
                        Self::expr_mentions_ident(chan, root)
                            || Self::expr_mentions_ident(value, root)
                            || Self::expr_has_boundary_using(chan, root)
                            || Self::expr_has_boundary_using(value, root)
                    }
                    SelectArmKind::Default => false,
                };
                let head_uses_name = match &a.kind {
                    SelectArmKind::Recv { chan, .. } => Self::expr_mentions_ident(chan, name),
                    SelectArmKind::Send { chan, value, .. } => {
                        Self::expr_mentions_ident(chan, name)
                            || Self::expr_mentions_ident(value, name)
                    }
                    SelectArmKind::Default => false,
                };
                head_has || in_block(&a.body, seen_loan_use || head_uses_name)
            }),
            _ => false,
        }
    }

    fn expr_has_boundary_using(e: &Expr, root: &str) -> bool {
        match e {
            Expr::Move { .. } => true,
            Expr::Call { callee, args, .. } => {
                let is_delete_or_clear = matches!(
                    callee.as_ref(),
                    Expr::Ident { name, .. } if name == "delete" || name == "clear"
                ) && !args.is_empty();
                is_delete_or_clear
                    || Self::expr_has_boundary_using(callee, root)
                    || args
                        .iter()
                        .any(|a| Self::expr_has_boundary_using(&a.value, root))
            }
            Expr::Binary { lhs, rhs, .. } => {
                Self::expr_has_boundary_using(lhs, root)
                    || Self::expr_has_boundary_using(rhs, root)
            }
            Expr::Unary { operand, .. }
            | Expr::AddrOf { operand, .. }
            | Expr::Try { operand, .. }
            | Expr::Receive { chan: operand, .. }
            | Expr::Field { base: operand, .. }
            | Expr::Paren { inner: operand, .. } => Self::expr_has_boundary_using(operand, root),
            Expr::Index { base, index, .. } => {
                Self::expr_has_boundary_using(base, root)
                    || Self::expr_has_boundary_using(index, root)
            }
            Expr::Slice { base, lo, hi, .. } => {
                Self::expr_has_boundary_using(base, root)
                    || lo
                        .as_deref()
                        .is_some_and(|x| Self::expr_has_boundary_using(x, root))
                    || hi
                        .as_deref()
                        .is_some_and(|x| Self::expr_has_boundary_using(x, root))
            }
            Expr::StructLit { fields, .. } => fields
                .iter()
                .any(|f| Self::expr_has_boundary_using(&f.value, root)),
            Expr::SliceLit { items, .. } => items
                .iter()
                .any(|x| Self::expr_has_boundary_using(x, root)),
            Expr::MapLit { entries, .. } => entries.iter().any(|(k, v)| {
                Self::expr_has_boundary_using(k, root) || Self::expr_has_boundary_using(v, root)
            }),
            Expr::Interp { parts, .. } => parts.iter().any(|p| match p {
                InterpPart::Lit(_) => false,
                InterpPart::Expr(x) => Self::expr_has_boundary_using(x, root),
            }),
            Expr::Ident { .. } | Expr::Lit { .. } | Expr::FnLit { .. } => false,
        }
    }

    /// Returns `true` if `stmts` may still read or mutate the current slot
    /// loan bound to `name`. A subsequent `name = &other_place` rebind (whose
    /// RHS does not itself mention `name`) starts a new loan rather than using
    /// the current loan.
    fn remaining_stmts_use_loan(stmts: &[Stmt], name: &str) -> bool {
        for st in stmts {
            match st {
                Stmt::Local(l) => {
                    if l.values.iter().any(|v| Self::expr_mentions_ident(v, name)) {
                        return true;
                    }
                    if l.names.iter().any(|n| n == name) {
                        return false;
                    }
                }
                Stmt::Assign {
                    targets,
                    op,
                    values,
                    ..
                } if *op == AssignOp::Assign
                    && targets.len() == values.len()
                    && targets
                        .iter()
                        .all(|t| matches!(&t.kind, PlaceKind::Ident(_)))
                    && values.iter().any(|v| Self::as_slot_loan_operand(v).is_some()) =>
                {
                    if values.iter().any(|v| Self::expr_mentions_ident(v, name)) {
                        return true;
                    }
                    if targets
                        .iter()
                        .any(|t| matches!(&t.kind, PlaceKind::Ident(n) if n == name))
                    {
                        return false;
                    }
                }
                _ => {
                    if Self::stmt_mentions_ident(st, name) {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn stmt_mentions_ident(st: &Stmt, target: &str) -> bool {
        match st {
            Stmt::Local(l) => l
                .values
                .iter()
                .any(|v| Self::expr_mentions_ident(v, target)),
            Stmt::Assign {
                targets, values, ..
            } => {
                targets
                    .iter()
                    .any(|t| Self::place_mentions_ident(t, target))
                    || values
                        .iter()
                        .any(|v| Self::expr_mentions_ident(v, target))
            }
            Stmt::Expr { expr, .. } => Self::expr_mentions_ident(expr, target),
            Stmt::Send { chan, value, .. } => {
                Self::expr_mentions_ident(chan, target)
                    || Self::expr_mentions_ident(value, target)
            }
            Stmt::Return { values, .. } => values
                .iter()
                .any(|v| Self::expr_mentions_ident(v, target)),
            Stmt::If {
                init,
                cond,
                then,
                otherwise,
                ..
            } => {
                // #547: an `if` init that DECLARES the target shadows it for
                // `cond`, both arms and `else` (Go's if-statement scope), so
                // those mentions are not uses of an outer binding of the same
                // name. The init's own value expressions still count — they
                // run before the binding exists.
                let init_mentions = init
                    .as_ref()
                    .is_some_and(|i| Self::stmt_mentions_ident(i, target));
                let shadows = init.as_ref().is_some_and(|i| Self::local_declares(i, target));
                init_mentions
                    || (!shadows
                        && (Self::expr_mentions_ident(cond, target)
                            || Self::block_mentions_ident(then, target)
                            || otherwise
                                .as_ref()
                                .is_some_and(|o| Self::stmt_mentions_ident(o, target))))
            }
            Stmt::For { kind, body, .. } => {
                let kind_mentions = match kind {
                    ForKind::Infinite => false,
                    ForKind::While { cond } => Self::expr_mentions_ident(cond, target),
                    ForKind::Range { lo, hi, .. } => {
                        Self::expr_mentions_ident(lo, target)
                            || Self::expr_mentions_ident(hi, target)
                    }
                    ForKind::Iterate { expr, .. } => Self::expr_mentions_ident(expr, target),
                };
                // #547: the head expressions run BEFORE the binder exists, so
                // they are uses of `target`; the BODY is not when the binder
                // re-declares `target`. `for n in 0..k { total += n }` under a
                // live slot loan `n` reads the LOOP variable, never the loan —
                // counting those reads stretched the loan's live range past a
                // following `move m`, which deferred the store-back `m[k] = n`
                // past the spawn (silent data race). A genuine re-use of the
                // outer `target` AFTER the loop is still reported by the
                // caller's remaining-statement scan, and the caller's `false`
                // only ever flushes the loan EARLY — it never drops a live one.
                kind_mentions
                    || (!Self::for_binder_shadows(kind, target)
                        && Self::block_mentions_ident(body, target))
            }
            Stmt::Match { subject, arms, .. } => {
                Self::expr_mentions_ident(subject, target)
                    || arms.iter().any(|a| {
                        let shadowed = matches!(
                            &a.pattern,
                            MatchPattern::Path {
                                binding: Some(b),
                                ..
                            } if b == target
                        );
                        !shadowed
                            && (a
                                .guard
                                .as_ref()
                                .is_some_and(|g| Self::expr_mentions_ident(g, target))
                                || Self::block_mentions_ident(&a.body, target))
                    })
            }
            Stmt::Select { arms, .. } => arms.iter().any(|a| {
                let head_mentions = match &a.kind {
                    SelectArmKind::Recv { chan, .. } => Self::expr_mentions_ident(chan, target),
                    SelectArmKind::Send { chan, value, .. } => {
                        Self::expr_mentions_ident(chan, target)
                            || Self::expr_mentions_ident(value, target)
                    }
                    SelectArmKind::Default => false,
                };
                // #547 (same class as the `for` binder): `n, ok := <-ch:` binds
                // a fresh `n` for the arm body; the channel expression is
                // evaluated before that binding exists.
                head_mentions
                    || (!Self::select_arm_binds(&a.kind, target)
                        && Self::block_mentions_ident(&a.body, target))
            }),
            // Counted conservatively: the deferred call's arguments are
            // evaluated at registration, and hy-check treats a deferred call's
            // captures as live, rejecting `defer` + `move` on the same arena
            // with HYB-E3003 (defer_move_soundness). Over-counting here keeps
            // the loan alive (fail-closed); it is not a binder, so #547 does
            // not touch this arm.
            Stmt::Defer { call, .. } => Self::expr_mentions_ident(call, target),
            Stmt::Task { callee, args, .. } => {
                Self::expr_mentions_ident(callee, target)
                    || args
                        .iter()
                        .any(|a| Self::expr_mentions_ident(&a.value, target))
            }
            Stmt::Managed { body, .. } | Stmt::Block { block: body, .. } => {
                Self::block_mentions_ident(body, target)
            }
            Stmt::Break { .. } | Stmt::Continue { .. } => false,
        }
    }

    /// Does this `Stmt` DECLARE `name` as a local? Only `Stmt::Local`
    /// introduces bindings; the other binder shapes (`for` heads, `select`
    /// recv names, `match` patterns) are not `Stmt`s and have their own
    /// predicates. Called for `if` inits only (#547).
    fn local_declares(st: &Stmt, name: &str) -> bool {
        matches!(st, Stmt::Local(l) if l.names.iter().any(|n| n == name))
    }

    /// Does this loop head bind `name`? A re-declaration shadows any outer
    /// binding of the same name inside the loop body only (#547).
    fn for_binder_shadows(kind: &ForKind, name: &str) -> bool {
        match kind {
            ForKind::Range { var, .. } => var == name,
            ForKind::Iterate { vars, .. } => vars.iter().any(|v| v == name),
            ForKind::Infinite | ForKind::While { .. } => false,
        }
    }

    /// Does this select arm's receive head bind `name`? (#547 — the arm body
    /// then reads the fresh binding, not an outer binding of the same name.)
    fn select_arm_binds(kind: &SelectArmKind, name: &str) -> bool {
        match kind {
            SelectArmKind::Recv { names, .. } => names.iter().any(|n| n == name),
            SelectArmKind::Send { .. } | SelectArmKind::Default => false,
        }
    }

    fn block_mentions_ident(b: &Block, target: &str) -> bool {
        Self::remaining_stmts_use_loan(&b.stmts, target)
    }

    fn place_mentions_ident(p: &Place, target: &str) -> bool {
        match &p.kind {
            PlaceKind::Ident(n) => n == target,
            PlaceKind::Field { base, .. } | PlaceKind::AddrOf { base } => {
                Self::place_mentions_ident(base, target)
            }
            PlaceKind::Index { base, index } => {
                Self::place_mentions_ident(base, target)
                    || Self::expr_mentions_ident(index, target)
            }
        }
    }

    fn expr_mentions_ident(e: &Expr, target: &str) -> bool {
        match e {
            Expr::Ident { name, .. } => name == target,
            Expr::Lit { .. } => false,
            Expr::Binary { lhs, rhs, .. } => {
                Self::expr_mentions_ident(lhs, target)
                    || Self::expr_mentions_ident(rhs, target)
            }
            Expr::Unary { operand, .. }
            | Expr::AddrOf { operand, .. }
            | Expr::Move { operand, .. }
            | Expr::Try { operand, .. }
            | Expr::Receive { chan: operand, .. }
            | Expr::Field { base: operand, .. }
            | Expr::Paren { inner: operand, .. } => Self::expr_mentions_ident(operand, target),
            Expr::Call { callee, args, .. } => {
                Self::expr_mentions_ident(callee, target)
                    || args
                        .iter()
                        .any(|a| Self::expr_mentions_ident(&a.value, target))
            }
            Expr::Index { base, index, .. } => {
                Self::expr_mentions_ident(base, target)
                    || Self::expr_mentions_ident(index, target)
            }
            Expr::Slice { base, lo, hi, .. } => {
                Self::expr_mentions_ident(base, target)
                    || lo
                        .as_ref()
                        .is_some_and(|x| Self::expr_mentions_ident(x, target))
                    || hi
                        .as_ref()
                        .is_some_and(|x| Self::expr_mentions_ident(x, target))
            }
            Expr::StructLit { fields, .. } => fields
                .iter()
                .any(|f| Self::expr_mentions_ident(&f.value, target)),
            Expr::SliceLit { items, .. } => {
                e.as_array_lit()
                    .and_then(|(len, _, _)| len)
                    .is_some_and(|l| Self::expr_mentions_ident(l, target))
                    || items
                        .iter()
                        .any(|x| Self::expr_mentions_ident(x, target))
            }
            Expr::MapLit { entries, .. } => entries.iter().any(|(k, v)| {
                Self::expr_mentions_ident(k, target) || Self::expr_mentions_ident(v, target)
            }),
            Expr::FnLit {
                params,
                results,
                body,
                ..
            } => {
                // #547: a closure binds its params AND its named results;
                // either one shadows an outer binding of the same name for the
                // body (`fn() (n int) { n = 5 }` under a live slot loan `n`
                // assigns the RESULT, never the loan).
                let binds = params.iter().any(|p| p.name == target)
                    || results
                        .iter()
                        .any(|r| r.name.as_deref() == Some(target));
                !binds && Self::block_mentions_ident(body, target)
            }
            Expr::Interp { parts, .. } => parts.iter().any(|p| match p {
                InterpPart::Lit(_) => false,
                InterpPart::Expr(x) => Self::expr_mentions_ident(x, target),
            }),
        }
    }

    fn stmt_modifies_ident(st: &Stmt, target: &str) -> bool {
        match st {
            Stmt::Local(l) => l.names.iter().any(|n| n == target),
            Stmt::Assign { targets, .. } => targets.iter().any(|t| match &t.kind {
                PlaceKind::Ident(n) => n == target,
                _ => false,
            }),
            Stmt::If {
                init,
                then,
                otherwise,
                ..
            } => {
                init.as_ref()
                    .is_some_and(|i| Self::stmt_modifies_ident(i, target))
                    || then
                        .stmts
                        .iter()
                        .any(|s| Self::stmt_modifies_ident(s, target))
                    || otherwise
                        .as_ref()
                        .is_some_and(|o| Self::stmt_modifies_ident(o, target))
            }
            Stmt::For { kind, body, .. } => {
                let head_modifies = match kind {
                    ForKind::Range { var, .. } => var == target,
                    ForKind::Iterate { vars, .. } => vars.iter().any(|v| v == target),
                    _ => false,
                };
                head_modifies
                    || body
                        .stmts
                        .iter()
                        .any(|s| Self::stmt_modifies_ident(s, target))
            }
            Stmt::Match { arms, .. } => arms.iter().any(|a| {
                a.body
                    .stmts
                    .iter()
                    .any(|s| Self::stmt_modifies_ident(s, target))
            }),
            Stmt::Select { arms, .. } => arms.iter().any(|a| {
                a.body
                    .stmts
                    .iter()
                    .any(|s| Self::stmt_modifies_ident(s, target))
            }),
            Stmt::Managed { body, .. } | Stmt::Block { block: body, .. } => body
                .stmts
                .iter()
                .any(|s| Self::stmt_modifies_ident(s, target)),
            _ => false,
        }
    }
}
