//! builtins.rs — defer-call shape + literal builtin-argument checks +
//! interprocedural / return-path `const` composite mutation checks (#429).
//!
//! Issue #187: programs that pass `hyb check` with 0 diagnostics but emit
//! Go that fails `go build` / `go vet`:
//!   * `defer` of a non-call (HYB-E2001 is registered but was never raised)
//!   * `make` / `len` / `cap` / `delete` / `close` / `new` with
//!     compile-time-detectable bad arity or literal misuse (HYB-E2005)
//!
//! Literal-only scope mirrors the landed #149 / #148 const-arith checks:
//! variable operands stay a run-time concern, matching Go.

use std::collections::{HashMap, HashSet};

use crate::model::{as_addr_of, collect_ref_roots, unwrap_parens, Checker, FnState, TypeShape};
use crate::walk::{self, CallRef};
use hy_diag::{Diagnostic, Span};
use hy_syntax::{
    Block, Expr, FnDecl, ForKind, LitKind, LocalKind, Param, Place, PlaceKind, SelectArmKind,
    Stmt, UnOp,
};

impl<'a> Checker<'a> {
    /// Pass-B entry: HYB-E2001 on `defer` + HYB-E2005 on builtin calls +
    /// #429 return-path & interprocedural `const` composite soundness checks.
    /// Nested block bodies are NOT recursed — `walk::flatten` already lists
    /// those statements (same discipline as `arith.rs`).
    pub(crate) fn check_stmt_builtins(&mut self, stmt: &Stmt) {
        self.check_stmt_defer_call(stmt);
        self.check_stmt_builtin_args_inner(stmt, /*deep=*/ false);
        self.check_stmt_const_soundness(stmt);
    }

    fn emit_const_write_error_dedup(&mut self, name: &str, span: Span) {
        if self
            .diags
            .iter()
            .any(|d| d.code == "HYB-E3006" && d.span == span)
        {
            return;
        }
        let msg = format!(
            "cannot write to `{}` — it is a `const` (deeply immutable)",
            name
        );
        let notes = vec![
            "`const` is a compile-time constant: no rebind, no element write, no field store through it (SPEC-LANGUAGE T22)".to_string(),
            "a file-level `const` lowers to package-level state every task body reaches ambiently — an accepted write would be a data race with zero spawn-site evidence (issue #37)".to_string(),
            "use a `let`/`var` local and cross task boundaries with a proof (`move`/send), or guard shared updates with `managed(\"why this is safe\", \"mutex\"|\"atomic\"|\"actor\")`".to_string(),
        ];
        self.diags.push(
            Diagnostic::error("HYB-E3006", self.path.clone(), span, msg).with_notes(notes),
        );
    }

    fn check_stmt_const_soundness(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Local(l) => {
                for v in &l.values {
                    self.check_expr_const_soundness(v);
                }
            }
            Stmt::Assign {
                targets, values, ..
            } => {
                for t in targets {
                    if let PlaceKind::Index { index, .. } = &t.kind {
                        self.check_expr_const_soundness(index);
                    }
                }
                for v in values {
                    self.check_expr_const_soundness(v);
                }
            }
            Stmt::Expr { expr, .. } => self.check_expr_const_soundness(expr),
            Stmt::Send { chan, value, .. } => {
                self.check_expr_const_soundness(chan);
                self.check_expr_const_soundness(value);
            }
            Stmt::If { cond, .. } => self.check_expr_const_soundness(cond),
            Stmt::For { kind, body, .. } => {
                match kind {
                    ForKind::While { cond } => self.check_expr_const_soundness(cond),
                    ForKind::Range { lo, hi, .. } => {
                        self.check_expr_const_soundness(lo);
                        self.check_expr_const_soundness(hi);
                    }
                    ForKind::Iterate { vars, expr } => {
                        self.check_expr_const_soundness(expr);
                        if let Some((const_name, _)) = self.expr_file_or_returned_const_root(expr) {
                            let empty = FnState::default();
                            let shapes = self.iterate_var_shapes(&empty, expr, vars.len());
                            for (v, sh) in vars.iter().zip(shapes) {
                                if v != "_"
                                    && (self.shape_may_share(&sh)
                                        || matches!(sh, TypeShape::Unknown))
                                {
                                    let mut al = HashSet::from([v.clone()]);
                                    let mut cl = HashMap::new();
                                    let mut spans = Vec::new();
                                    self.collect_const_param_mutation_spans_block(
                                        body,
                                        &mut al,
                                        &mut cl,
                                        &mut spans,
                                        0,
                                    );
                                    for sp in spans {
                                        self.emit_const_write_error_dedup(&const_name, sp);
                                    }
                                }
                            }
                        }
                    }
                    ForKind::Infinite => {}
                }
            }
            Stmt::Return { values, .. } => {
                for v in values {
                    self.check_expr_const_soundness(v);
                }
            }
            Stmt::Task { callee, args, .. } => {
                self.check_expr_const_soundness(callee);
                for a in args {
                    self.check_expr_const_soundness(&a.value);
                }
                let cr = CallRef {
                    callee,
                    args,
                    span: callee.span(),
                };
                self.check_call_const_soundness(&cr);
            }
            Stmt::Match { subject, arms, .. } => {
                self.check_expr_const_soundness(subject);
                for arm in arms {
                    if let Some(g) = &arm.guard {
                        self.check_expr_const_soundness(g);
                    }
                }
            }
            Stmt::Defer { call, .. } => self.check_expr_const_soundness(call),
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    match &arm.kind {
                        SelectArmKind::Send { chan, value } => {
                            self.check_expr_const_soundness(chan);
                            self.check_expr_const_soundness(value);
                        }
                        SelectArmKind::Recv { chan, .. } => self.check_expr_const_soundness(chan),
                        SelectArmKind::Default => {}
                    }
                }
            }
            Stmt::Managed { .. }
            | Stmt::Block { .. }
            | Stmt::Break { .. }
            | Stmt::Continue { .. } => {}
        }
    }

    fn check_expr_const_soundness(&mut self, expr: &Expr) {
        match expr {
            Expr::Call { callee, args, span } => {
                self.check_expr_const_soundness(callee);
                for a in args {
                    self.check_expr_const_soundness(&a.value);
                }
                let cr = CallRef {
                    callee,
                    args,
                    span: *span,
                };
                self.check_call_const_soundness(&cr);
            }
            Expr::FnLit { body, .. } => {
                let mut al = HashMap::new();
                self.scan_closure_const_return_writes(body, &mut al);
            }
            Expr::Binary { lhs, rhs, .. } | Expr::Index { base: lhs, index: rhs, .. } => {
                self.check_expr_const_soundness(lhs);
                self.check_expr_const_soundness(rhs);
            }
            Expr::Unary { operand, .. }
            | Expr::AddrOf { operand, .. }
            | Expr::Move { operand, .. }
            | Expr::Try { operand, .. }
            | Expr::Paren { inner: operand, .. }
            | Expr::Receive { chan: operand, .. }
            | Expr::Field { base: operand, .. } => self.check_expr_const_soundness(operand),
            Expr::Slice { base, lo, hi, .. } => {
                self.check_expr_const_soundness(base);
                if let Some(lo) = lo {
                    self.check_expr_const_soundness(lo);
                }
                if let Some(hi) = hi {
                    self.check_expr_const_soundness(hi);
                }
            }
            Expr::StructLit { fields, .. } => {
                for f in fields {
                    self.check_expr_const_soundness(&f.value);
                }
            }
            Expr::SliceLit { items, .. } => {
                for it in items {
                    self.check_expr_const_soundness(it);
                }
            }
            Expr::MapLit { entries, .. } => {
                for (k, v) in entries {
                    self.check_expr_const_soundness(k);
                    self.check_expr_const_soundness(v);
                }
            }
            Expr::Interp { parts, .. } => {
                for p in parts {
                    if let hy_syntax::InterpPart::Expr(e) = p {
                        self.check_expr_const_soundness(e);
                    }
                }
            }
            Expr::Ident { .. } | Expr::Lit { .. } => {}
        }
    }

    fn scan_closure_const_return_writes(
        &mut self,
        block: &Block,
        const_locals: &mut HashMap<String, String>,
    ) {
        for s in &block.stmts {
            match s {
                Stmt::Local(l) => {
                    for v in &l.values {
                        self.check_expr_const_soundness(v);
                    }
                    if l.names.len() == l.values.len() {
                        for (name, val) in l.names.iter().zip(l.values.iter()) {
                            if let Some((cname, _)) = self.expr_file_or_returned_const_root(val) {
                                const_locals.insert(name.clone(), cname);
                            } else {
                                const_locals.remove(name);
                            }
                        }
                    } else if l.values.len() == 1 && l.names.len() > 1 {
                        for (i, name) in l.names.iter().enumerate() {
                            if let Some((cname, _)) =
                                self.expr_returned_const_root(None, &l.values[0], i, 0)
                            {
                                const_locals.insert(name.clone(), cname);
                            } else {
                                const_locals.remove(name);
                            }
                        }
                    }
                }
                Stmt::Assign {
                    targets, values, ..
                } => {
                    for v in values {
                        self.check_expr_const_soundness(v);
                    }
                    for t in targets {
                        if matches!(t.kind, PlaceKind::Index { .. } | PlaceKind::Field { .. }) {
                            if let Some(cname) = const_locals.get(t.root()) {
                                self.emit_const_write_error_dedup(cname, t.span);
                            }
                        }
                    }
                    if targets.len() == values.len() {
                        for (t, v) in targets.iter().zip(values.iter()) {
                            if let PlaceKind::Ident(name) = &t.kind {
                                if let Some((cname, _)) = self.expr_file_or_returned_const_root(v) {
                                    const_locals.insert(name.clone(), cname);
                                } else {
                                    const_locals.remove(name);
                                }
                            }
                        }
                    }
                }
                Stmt::If {
                    then, otherwise, ..
                } => {
                    let mut then_map = const_locals.clone();
                    self.scan_closure_const_return_writes(then, &mut then_map);
                    if let Some(Stmt::Block { block: ob, .. }) = otherwise.as_deref() {
                        let mut else_map = const_locals.clone();
                        self.scan_closure_const_return_writes(ob, &mut else_map);
                    }
                }
                Stmt::For { body, .. }
                | Stmt::Managed { body, .. }
                | Stmt::Block { block: body, .. } => {
                    let mut inner_map = const_locals.clone();
                    self.scan_closure_const_return_writes(body, &mut inner_map);
                }
                _ => {
                    self.check_stmt_const_soundness(s);
                }
            }
        }
    }

    fn check_call_const_soundness(&mut self, cr: &CallRef) {
        let callee = unwrap_parens(cr.callee);
        if let Expr::Ident { name, .. } = callee {
            if name == "delete" || name == "copy" {
                if let Some(first) = cr.args.first() {
                    if let Some((const_name, _)) =
                        self.expr_returned_const_root(None, &first.value, 0, 0)
                    {
                        self.emit_const_write_error_dedup(&const_name, first.value.span());
                    }
                }
                return;
            }
        }

        let mut hits: Vec<(String, Span)> = Vec::new();
        if let Expr::Field {
            base,
            name: method_name,
            ..
        } = callee
        {
            if let Some((const_name, _)) = self.expr_file_or_returned_const_root(base) {
                let base_method = method_name
                    .split('[')
                    .next()
                    .unwrap_or(method_name.as_str())
                    .trim();
                for ((_, mname), decl) in &self.method_decls {
                    if mname == base_method {
                        if let Some(r) = &decl.receiver {
                            if self.fn_mutates_const_param_ext(decl, &r.name, 0) {
                                hits.push((const_name.clone(), base.span()));
                            }
                        }
                    }
                }
            }
        }

        for (i, a) in cr.args.iter().enumerate() {
            let Some((const_name, _)) = self.expr_file_or_returned_const_root(&a.value) else {
                continue;
            };
            let mutates = match callee {
                Expr::Ident { name, .. } => {
                    let base_name = name.split('[').next().unwrap_or(name.as_str()).trim();
                    if let Some(decl) = self.fns.get(base_name) {
                        decl.params
                            .get(i)
                            .map(|p| self.fn_mutates_const_param_ext(decl, &p.name, 0))
                            .unwrap_or(false)
                    } else if let Some((params, _, body)) =
                        self.find_local_closure_parts_in_file(base_name)
                    {
                        params
                            .get(i)
                            .map(|p| {
                                let mut al = HashSet::from([p.name.clone()]);
                                let mut cl = HashMap::new();
                                self.block_mutates_const_param_ext(body, &mut al, &mut cl, 0)
                            })
                            .unwrap_or(false)
                    } else {
                        false
                    }
                }
                Expr::Field {
                    name: method_name, ..
                } => {
                    let base_method = method_name
                        .split('[')
                        .next()
                        .unwrap_or(method_name.as_str())
                        .trim();
                    self.method_decls.iter().any(|((_, mname), decl)| {
                        mname == base_method
                            && decl
                                .params
                                .get(i)
                                .map(|p| self.fn_mutates_const_param_ext(decl, &p.name, 0))
                                .unwrap_or(false)
                    })
                }
                Expr::FnLit { params, body, .. } => params
                    .get(i)
                    .map(|p| {
                        let mut al = HashSet::from([p.name.clone()]);
                        let mut cl = HashMap::new();
                        self.block_mutates_const_param_ext(body, &mut al, &mut cl, 0)
                    })
                    .unwrap_or(false),
                _ => false,
            };
            if mutates {
                hits.push((const_name, a.value.span()));
            }
        }

        for (const_name, span) in hits {
            self.emit_const_write_error_dedup(&const_name, span);
        }
    }

    fn expr_file_or_returned_const_root(&self, expr: &Expr) -> Option<(String, String)> {
        if let Some(hit) = self.expr_direct_or_returned_const_root(None, expr, 0, 0) {
            return Some(hit);
        }
        let inner = unwrap_parens(expr);
        if let Some(addr_op) = as_addr_of(inner) {
            if let Some(root) = addr_op.root_ident() {
                if let Some(cb) = self.consts.get(root) {
                    let orig = cb
                        .non_owned_origin
                        .clone()
                        .unwrap_or_else(|| format!("file-level `const {}`", root));
                    return Some((root.to_string(), orig));
                }
            }
        }
        if let Expr::Ident { name, .. } = inner {
            if let Some(hit) = self.resolve_local_const_binding_in_file(name) {
                return Some(hit);
            }
        }
        None
    }

    fn resolve_local_const_binding_in_file(&self, target_name: &str) -> Option<(String, String)> {
        for f in self.file.fns() {
            if let Some(body) = &f.body {
                if let Some(hit) = self.find_local_const_in_block(body, target_name) {
                    return Some(hit);
                }
            }
        }
        None
    }

    fn find_local_const_in_block(
        &self,
        block: &Block,
        target_name: &str,
    ) -> Option<(String, String)> {
        for s in &block.stmts {
            match s {
                Stmt::Local(l) => {
                    if l.kind == LocalKind::Const {
                        for (n, v) in l.names.iter().zip(l.values.iter()) {
                            if n == target_name {
                                let empty = FnState::default();
                                let sh = l
                                    .ty
                                    .as_ref()
                                    .map(|t| self.shape_of_type(t))
                                    .unwrap_or_else(|| self.shape_of_expr(&empty, v));
                                if self.shape_may_share(&sh) || matches!(sh, TypeShape::Unknown) {
                                    return Some((n.clone(), format!("`const {}`", n)));
                                }
                            }
                        }
                    } else if l.names.len() == l.values.len() {
                        for (n, v) in l.names.iter().zip(l.values.iter()) {
                            if n == target_name {
                                if let Some(hit) =
                                    self.expr_direct_or_returned_const_root(None, v, 0, 0)
                                {
                                    return Some(hit);
                                }
                            }
                        }
                    } else if l.values.len() == 1 && l.names.len() > 1 {
                        for (i, n) in l.names.iter().enumerate() {
                            if n == target_name {
                                if let Some(hit) = self.expr_direct_or_returned_const_root(
                                    None,
                                    &l.values[0],
                                    i,
                                    0,
                                ) {
                                    return Some(hit);
                                }
                            }
                        }
                    }
                }
                Stmt::If {
                    then, otherwise, ..
                } => {
                    if let Some(h) = self.find_local_const_in_block(then, target_name) {
                        return Some(h);
                    }
                    if let Some(Stmt::Block { block: ob, .. }) = otherwise.as_deref() {
                        if let Some(h) = self.find_local_const_in_block(ob, target_name) {
                            return Some(h);
                        }
                    }
                }
                Stmt::For { body, .. }
                | Stmt::Managed { body, .. }
                | Stmt::Block { block: body, .. } => {
                    if let Some(h) = self.find_local_const_in_block(body, target_name) {
                        return Some(h);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn fn_mutates_const_param_ext(&self, decl: &'a FnDecl, param: &str, depth: usize) -> bool {
        if depth > 4 {
            return true;
        }
        let Some(body) = &decl.body else {
            return false;
        };
        let mut aliases = HashSet::new();
        aliases.insert(param.to_string());
        let mut closures = HashMap::new();
        self.block_mutates_const_param_ext(body, &mut aliases, &mut closures, depth)
    }

    fn block_mutates_const_param_ext(
        &self,
        block: &'a Block,
        aliases: &mut HashSet<String>,
        closures: &mut HashMap<String, (&'a [Param], &'a Block)>,
        depth: usize,
    ) -> bool {
        let mut spans = Vec::new();
        self.collect_const_param_mutation_spans_block(block, aliases, closures, &mut spans, depth);
        !spans.is_empty()
    }

    fn collect_const_param_mutation_spans_block(
        &self,
        block: &'a Block,
        aliases: &mut HashSet<String>,
        closures: &mut HashMap<String, (&'a [Param], &'a Block)>,
        spans: &mut Vec<Span>,
        depth: usize,
    ) {
        for s in &block.stmts {
            self.collect_const_param_mutation_spans_stmt(s, aliases, closures, spans, depth);
        }
    }

    fn expr_reaches_const_aliases_ext(expr: &Expr, aliases: &HashSet<String>) -> bool {
        let inner = unwrap_parens(expr);
        if let Some(addr_op) = as_addr_of(inner) {
            if let Some(r) = addr_op.root_ident() {
                if aliases.contains(r) {
                    return true;
                }
            }
        }
        if let Some(r) = inner.root_ident() {
            if aliases.contains(r) {
                return true;
            }
        }
        let mut roots = Vec::new();
        collect_ref_roots(inner, &mut roots);
        roots.iter().any(|r| aliases.contains(r))
    }

    fn multi_value_slot_reaches_const_aliases(
        &self,
        rhs: &Expr,
        slot_idx: usize,
        total_slots: usize,
        aliases: &HashSet<String>,
    ) -> bool {
        if !Self::expr_reaches_const_aliases_ext(rhs, aliases) {
            return false;
        }
        let inner = unwrap_parens(rhs);
        match inner {
            Expr::Index { .. } | Expr::Receive { .. } => slot_idx == 0,
            Expr::Call { .. } => {
                let empty = FnState::default();
                let shapes = self.multi_value_shapes(&empty, inner, total_slots);
                shapes
                    .get(slot_idx)
                    .map(|sh| self.shape_may_share(sh) || matches!(sh, TypeShape::Unknown))
                    .unwrap_or(true)
            }
            _ => slot_idx == 0,
        }
    }

    fn collect_const_param_mutation_spans_stmt(
        &self,
        stmt: &'a Stmt,
        aliases: &mut HashSet<String>,
        closures: &mut HashMap<String, (&'a [Param], &'a Block)>,
        spans: &mut Vec<Span>,
        depth: usize,
    ) {
        match stmt {
            Stmt::Assign {
                targets, values, ..
            } => {
                for t in targets {
                    if matches!(t.kind, PlaceKind::Index { .. } | PlaceKind::Field { .. })
                        && aliases.contains(t.root())
                    {
                        spans.push(t.span);
                    }
                }
                for v in values {
                    self.collect_expr_fnlit_const_param_mutations(v, aliases, closures, spans, depth);
                }
                if targets.len() == values.len() {
                    for (t, v) in targets.iter().zip(values.iter()) {
                        if let PlaceKind::Ident(name) = &t.kind {
                            if let Expr::FnLit { params, body, .. } = unwrap_parens(v) {
                                closures.insert(name.clone(), (params.as_slice(), body));
                            }
                            if Self::expr_reaches_const_aliases_ext(v, aliases) {
                                aliases.insert(name.clone());
                            } else {
                                aliases.remove(name);
                            }
                        }
                    }
                } else if values.len() == 1 && targets.len() > 1 {
                    let mut updates = Vec::new();
                    for (idx, t) in targets.iter().enumerate() {
                        if let PlaceKind::Ident(name) = &t.kind {
                            let reaches = self.multi_value_slot_reaches_const_aliases(
                                &values[0],
                                idx,
                                targets.len(),
                                aliases,
                            );
                            updates.push((name.clone(), reaches));
                        }
                    }
                    for (name, reaches) in updates {
                        if reaches {
                            aliases.insert(name);
                        } else {
                            aliases.remove(&name);
                        }
                    }
                }
            }
            Stmt::Local(l) => {
                for v in &l.values {
                    self.collect_expr_fnlit_const_param_mutations(v, aliases, closures, spans, depth);
                }
                if l.values.is_empty() {
                    for name in &l.names {
                        aliases.remove(name);
                        closures.remove(name);
                    }
                } else if l.names.len() == l.values.len() {
                    for (name, v) in l.names.iter().zip(l.values.iter()) {
                        if let Expr::FnLit { params, body, .. } = unwrap_parens(v) {
                            closures.insert(name.clone(), (params.as_slice(), body));
                        }
                        if Self::expr_reaches_const_aliases_ext(v, aliases) {
                            aliases.insert(name.clone());
                        } else {
                            aliases.remove(name);
                        }
                    }
                } else if l.values.len() == 1 && l.names.len() > 1 {
                    let mut updates = Vec::new();
                    for (idx, name) in l.names.iter().enumerate() {
                        let reaches = self.multi_value_slot_reaches_const_aliases(
                            &l.values[0],
                            idx,
                            l.names.len(),
                            aliases,
                        );
                        updates.push((name.clone(), reaches));
                    }
                    for (name, reaches) in updates {
                        if reaches {
                            aliases.insert(name);
                        } else {
                            aliases.remove(&name);
                        }
                    }
                }
            }
            Stmt::Send { value, span, .. } => {
                if Self::expr_reaches_const_aliases_ext(value, aliases) {
                    spans.push(*span);
                }
            }
            Stmt::Task { callee, args, span } => {
                if args
                    .iter()
                    .any(|a| Self::expr_reaches_const_aliases_ext(&a.value, aliases))
                {
                    spans.push(*span);
                }
                let cr = CallRef {
                    callee,
                    args,
                    span: *span,
                };
                if self.call_mutates_const_param_ext(&cr, aliases, closures, depth) {
                    spans.push(*span);
                }
            }
            _ => {}
        }

        let mut calls = Vec::new();
        walk::own_calls(stmt, &mut calls);
        for cr in &calls {
            if self.call_mutates_const_param_ext(cr, aliases, closures, depth) {
                spans.push(cr.span);
            }
        }

        match stmt {
            Stmt::If {
                init,
                then,
                otherwise,
                ..
            } => {
                let mut branch_aliases = aliases.clone();
                let mut branch_closures = closures.clone();
                if let Some(i) = init {
                    self.collect_const_param_mutation_spans_stmt(
                        i,
                        &mut branch_aliases,
                        &mut branch_closures,
                        spans,
                        depth,
                    );
                }
                let mut then_aliases = branch_aliases.clone();
                let mut then_closures = branch_closures.clone();
                self.collect_const_param_mutation_spans_block(
                    then,
                    &mut then_aliases,
                    &mut then_closures,
                    spans,
                    depth,
                );
                if let Some(o) = otherwise {
                    let mut else_aliases = branch_aliases;
                    let mut else_closures = branch_closures;
                    self.collect_const_param_mutation_spans_stmt(
                        o,
                        &mut else_aliases,
                        &mut else_closures,
                        spans,
                        depth,
                    );
                }
            }
            Stmt::For { kind, body, .. } => {
                let mut body_aliases = aliases.clone();
                let mut body_closures = closures.clone();
                if let ForKind::Iterate { vars, expr } = kind {
                    if Self::expr_reaches_const_aliases_ext(expr, aliases) {
                        for v in vars {
                            if v != "_" {
                                body_aliases.insert(v.clone());
                            }
                        }
                    } else {
                        for v in vars {
                            body_aliases.remove(v);
                        }
                    }
                }
                self.collect_const_param_mutation_spans_block(
                    body,
                    &mut body_aliases,
                    &mut body_closures,
                    spans,
                    depth,
                );
            }
            Stmt::Managed { body, .. } | Stmt::Block { block: body, .. } => {
                let mut body_aliases = aliases.clone();
                let mut body_closures = closures.clone();
                self.collect_const_param_mutation_spans_block(
                    body,
                    &mut body_aliases,
                    &mut body_closures,
                    spans,
                    depth,
                );
            }
            Stmt::Match { arms, .. } => {
                for arm in arms {
                    let mut arm_aliases = aliases.clone();
                    let mut arm_closures = closures.clone();
                    self.collect_const_param_mutation_spans_block(
                        &arm.body,
                        &mut arm_aliases,
                        &mut arm_closures,
                        spans,
                        depth,
                    );
                }
            }
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    if let SelectArmKind::Send { value, .. } = &arm.kind {
                        if Self::expr_reaches_const_aliases_ext(value, aliases) {
                            spans.push(value.span());
                        }
                    }
                    let mut arm_aliases = aliases.clone();
                    let mut arm_closures = closures.clone();
                    if let SelectArmKind::Recv { names, chan } = &arm.kind {
                        if Self::expr_reaches_const_aliases_ext(chan, aliases) {
                            if let Some(first) = names.first() {
                                arm_aliases.insert(first.clone());
                            }
                        }
                    }
                    self.collect_const_param_mutation_spans_block(
                        &arm.body,
                        &mut arm_aliases,
                        &mut arm_closures,
                        spans,
                        depth,
                    );
                }
            }
            _ => {}
        }
    }

    fn collect_expr_fnlit_const_param_mutations(
        &self,
        e: &'a Expr,
        aliases: &HashSet<String>,
        closures: &HashMap<String, (&'a [Param], &'a Block)>,
        spans: &mut Vec<Span>,
        depth: usize,
    ) {
        match e {
            Expr::FnLit { params, body, .. } => {
                let mut inner_aliases = aliases.clone();
                for p in params {
                    inner_aliases.remove(&p.name);
                }
                if !inner_aliases.is_empty() {
                    let mut inner_closures = closures.clone();
                    self.collect_const_param_mutation_spans_block(
                        body,
                        &mut inner_aliases,
                        &mut inner_closures,
                        spans,
                        depth,
                    );
                }
            }
            Expr::Paren { inner, .. }
            | Expr::Move { operand: inner, .. }
            | Expr::Try { operand: inner, .. }
            | Expr::Unary { operand: inner, .. }
            | Expr::AddrOf { operand: inner, .. } => {
                self.collect_expr_fnlit_const_param_mutations(inner, aliases, closures, spans, depth)
            }
            Expr::Call { callee, args, .. } => {
                self.collect_expr_fnlit_const_param_mutations(callee, aliases, closures, spans, depth);
                for a in args {
                    self.collect_expr_fnlit_const_param_mutations(
                        &a.value, aliases, closures, spans, depth,
                    );
                }
            }
            Expr::Index { base, index, .. }
            | Expr::Binary {
                lhs: base,
                rhs: index,
                ..
            } => {
                self.collect_expr_fnlit_const_param_mutations(base, aliases, closures, spans, depth);
                self.collect_expr_fnlit_const_param_mutations(index, aliases, closures, spans, depth);
            }
            _ => {}
        }
    }

    fn call_mutates_const_param_ext(
        &self,
        cr: &CallRef,
        aliases: &HashSet<String>,
        closures: &HashMap<String, (&'a [Param], &'a Block)>,
        depth: usize,
    ) -> bool {
        if depth > 4 {
            return true;
        }
        let arg_reaches = |a: &hy_syntax::CallArg| Self::expr_reaches_const_aliases_ext(&a.value, aliases);
        match unwrap_parens(cr.callee) {
            Expr::Ident { name, .. } => {
                if name == "delete" || name == "copy" {
                    return cr.args.first().map(&arg_reaches).unwrap_or(false);
                }
                if matches!(
                    name.as_str(),
                    "len" | "cap" | "make" | "new" | "append" | "print" | "println" | "string"
                ) {
                    return false;
                }
                if let Some((c_params, c_body)) = closures.get(name.as_str()) {
                    let mut inner_aliases = aliases.clone();
                    for (i, p) in c_params.iter().enumerate() {
                        if cr.args.get(i).map(&arg_reaches).unwrap_or(false) {
                            inner_aliases.insert(p.name.clone());
                        } else {
                            inner_aliases.remove(&p.name);
                        }
                    }
                    let mut inner_closures = closures.clone();
                    return self.block_mutates_const_param_ext(
                        c_body,
                        &mut inner_aliases,
                        &mut inner_closures,
                        depth + 1,
                    );
                }
                let base_name = name.split('[').next().unwrap_or(name.as_str()).trim();
                if let Some(decl) = self.fns.get(base_name) {
                    for (i, a) in cr.args.iter().enumerate() {
                        if !arg_reaches(a) {
                            continue;
                        }
                        match decl.params.get(i) {
                            Some(p) => {
                                if self.fn_mutates_const_param_ext(decl, &p.name, depth + 1) {
                                    return true;
                                }
                            }
                            None => return true,
                        }
                    }
                    return false;
                }
                cr.args.iter().any(&arg_reaches)
            }
            Expr::Field {
                base,
                name: method_name,
                ..
            } => {
                if let Expr::Ident { name: pkg, .. } = unwrap_parens(base) {
                    if matches!(
                        pkg.as_str(),
                        "fmt" | "strings" | "strconv" | "math" | "time" | "bytes" | "sort"
                    ) {
                        return false;
                    }
                }
                let base_method = method_name
                    .split('[')
                    .next()
                    .unwrap_or(method_name.as_str())
                    .trim();
                let mut found_method = false;
                for ((_, mname), decl) in &self.method_decls {
                    if mname == base_method {
                        found_method = true;
                        if Self::expr_reaches_const_aliases_ext(base, aliases) {
                            if let Some(r) = &decl.receiver {
                                if self.fn_mutates_const_param_ext(decl, &r.name, depth + 1) {
                                    return true;
                                }
                            }
                        }
                        for (i, a) in cr.args.iter().enumerate() {
                            if !arg_reaches(a) {
                                continue;
                            }
                            if let Some(p) = decl.params.get(i) {
                                if self.fn_mutates_const_param_ext(decl, &p.name, depth + 1) {
                                    return true;
                                }
                            }
                        }
                    }
                }
                if found_method {
                    false
                } else {
                    cr.args.iter().any(&arg_reaches)
                }
            }
            Expr::FnLit { params, body, .. } => {
                let mut inner_aliases = aliases.clone();
                for (i, p) in params.iter().enumerate() {
                    if cr.args.get(i).map(&arg_reaches).unwrap_or(false) {
                        inner_aliases.insert(p.name.clone());
                    } else {
                        inner_aliases.remove(&p.name);
                    }
                }
                let mut inner_closures = closures.clone();
                self.block_mutates_const_param_ext(
                    body,
                    &mut inner_aliases,
                    &mut inner_closures,
                    depth + 1,
                )
            }
            _ => cr.args.iter().any(&arg_reaches),
        }
    }

    fn check_stmt_defer_call(&mut self, stmt: &Stmt) {
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
                        SelectArmKind::Send { chan, value } => {
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
            PlaceKind::Index { index, .. } => self.check_expr_builtin_args(index),
            PlaceKind::Field { .. } | PlaceKind::Ident(_) | PlaceKind::AddrOf { .. } => {}
        }
    }

    fn check_expr_builtin_args(&mut self, expr: &Expr) {
        match expr {
            Expr::Call { callee, args, span } => {
                self.check_expr_builtin_args(callee);
                for a in args {
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
            Expr::StructLit { fields, .. } => {
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
            // FnLit bodies are the A5 perimeter — not walked here.
            Expr::FnLit { .. } | Expr::Ident { .. } | Expr::Lit { .. } => {}
        }
    }

    fn check_builtin_call(&mut self, name: &str, args: &[hy_syntax::CallArg], span: Span) {
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
            _ => {}
        }
    }

    fn check_make_args(&mut self, args: &[hy_syntax::CallArg], span: Span) {
        // `make([]T, len, cap)` — the 3-argument form — is valid Go for slices
        // only (issue #353). The parser encodes a type in expression position
        // as a literal node (`[]T` → SliceLit with no items); every other
        // type spelling keeps the 1–2 argument limit.
        let slice_form = args.len() == 3
            && matches!(
                args.first().map(|a| unwrap_parens(&a.value)),
                Some(Expr::SliceLit { items, .. }) if items.is_empty()
            );
        let max_args = if slice_form { 3 } else { 2 };
        if args.is_empty() || args.len() > max_args {
            self.emit_builtin(
                span,
                format!(
                    "`make` requires 1 or 2 arguments (type[, size]) — or 3 for a slice (type, len, cap) — found {} (HYB-E2005)",
                    args.len()
                ),
            );
            return;
        }
        for arg in args.iter().skip(1) {
            match classify_make_size(&arg.value) {
                MakeSize::Ok | MakeSize::NonLiteral => {}
                MakeSize::Negative => self.emit_builtin(
                    span,
                    "`make` size must not be a negative constant (HYB-E2005)".to_string(),
                ),
                MakeSize::BadLiteral(kind) => self.emit_builtin(
                    span,
                    format!(
                        "`make` size must be a non-negative integer constant or a variable — not a {kind} literal (HYB-E2005)"
                    ),
                ),
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

enum MakeSize {
    Ok,
    NonLiteral,
    Negative,
    BadLiteral(&'static str),
}

fn classify_make_size(e: &Expr) -> MakeSize {
    let e = unwrap_parens(e);
    match e {
        Expr::Lit { kind, .. } => match kind {
            LitKind::Int(s) => match s.parse::<i128>() {
                Ok(n) if n >= 0 => MakeSize::Ok,
                Ok(_) => MakeSize::Negative,
                Err(_) => MakeSize::BadLiteral("integer"),
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
        } => {
            // `-1`, `-0` — a negated int literal is a negative constant (or zero).
            match unwrap_parens(operand) {
                Expr::Lit {
                    kind: LitKind::Int(_),
                    ..
                } => MakeSize::Negative,
                Expr::Lit {
                    kind: LitKind::Float(_),
                    ..
                } => MakeSize::BadLiteral("float"),
                _ => MakeSize::NonLiteral,
            }
        }
        _ => MakeSize::NonLiteral,
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

#[cfg(test)]
mod tests {
    use crate::{check_file, Verdict};
    use hy_syntax::parse_file;

    fn check(src: &str) -> crate::CheckResult {
        let f = parse_file("test.hy", src).expect("parse");
        check_file(&f)
    }

    #[test]
    fn fn_returning_const_composite_rejects_write_move_and_delete_issue_429() {
        let r = check(
            "const shared = map[string]int{\"k\": 0}\n\
             fn get_shared() map[string]int {\n\
               return shared\n\
             }\n\
             fn worker_ambient(done chan int) {\n\
               m := get_shared()\n\
               for i in 0..10 {\n\
                 m[\"k\"] = i\n\
               }\n\
               done <- 1\n\
             }\n\
             fn worker_moved(m map[string]int, done chan int) {\n\
               for i in 0..10 {\n\
                 m[\"k\"] = i\n\
               }\n\
               done <- 1\n\
             }\n\
             fn main() {\n\
               done := make(chan int, 2)\n\
               task worker_ambient(done)\n\
               m0 := get_shared()\n\
               for i in 0..10 {\n\
                 m0[\"k\"] = i\n\
               }\n\
               _ = <-done\n\
               m1 := get_shared()\n\
               m2 := get_shared()\n\
               task worker_moved(move m1, done)\n\
               for i in 0..10 {\n\
                 m2[\"k\"] = i\n\
               }\n\
               delete(get_shared(), \"k\")\n\
               _ = <-done\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
            "expected HYB-E3006 on const-returning call mutation, got {:?}",
            r.diagnostics
        );
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
            "expected HYB-E3004 on moving const-returning call binding, got {:?}",
            r.diagnostics
        );
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
            "expected HYB-E3003 on post-move peer write of const-returning call binding, got {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn interproc_const_param_iter_comma_ok_and_post_rebind_reject_issue_429() {
        let r = check(
            "struct holder {\n\
               m map[string]int\n\
             }\n\
             const shared_list = [holder{m: map[string]int{\"k\": 0}}]\n\
             const shared_nested = map[string]map[string]int{\"a\": map[string]int{\"k\": 0}}\n\
             const shared_map = map[string]int{\"k\": 0}\n\
             fn mutate_iter(xs []holder) {\n\
               for _, h in xs {\n\
                 h.m[\"k\"] = 1\n\
               }\n\
             }\n\
             fn mutate_comma_ok(outer map[string]map[string]int) {\n\
               inner, ok := outer[\"a\"]\n\
               if ok {\n\
                 inner[\"k\"] = 1\n\
               }\n\
             }\n\
             fn mutate_post_rebind(m map[string]int) {\n\
               var m2 map[string]int = map[string]int{\"k\": 0}\n\
               _ = m2\n\
               f := fn() {\n\
                 m2[\"k\"] = 2\n\
               }\n\
               m2 = m\n\
               f()\n\
             }\n\
             fn main() {\n\
               mutate_comma_ok(shared_nested)\n\
               mutate_post_rebind(shared_map)\n\
               mutate_iter(shared_list)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        let e3006_count = r.diagnostics.iter().filter(|d| d.code == "HYB-E3006").count();
        assert_eq!(
            e3006_count, 3,
            "expected 3 HYB-E3006 diagnostics for comma-ok, post-rebind closure, and for-in iteration, got {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn readonly_const_return_and_paren_const_accept_issue_429() {
        let r = check(
            "const shared = (map[string]int{\"k\": 1})\n\
             fn get_shared() map[string]int {\n\
               return shared\n\
             }\n\
             fn main() {\n\
               const local_paren = ([1, 2])\n\
               m := get_shared()\n\
               _ = m[\"k\"]\n\
               _ = local_paren[0]\n\
               _ = shared[\"k\"]\n\
               _ = local_paren[0]\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
    }
}
