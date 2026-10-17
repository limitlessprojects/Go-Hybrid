//! addr_loan.rs — T24: no address-of on a loan (HYB-E3104), plus return-path
//! `const` composite origin propagation (#429).
//!
//! SPEC-LANGUAGE.md §2.9 (T24) pins a compile-fail claim:
//!
//! ```hy
//! n := &g.nodes[from]      // ok: map-slot loan (O8) — the only general-value `&`
//! p := &n                  // reject: HYB-E3104 — no address-of on a loan
//! ```
//!
//! Before this pass the second line was accepted with 0 diagnostics: the
//! emitter lowered the loan to a copy (`n := g.nodes[from]`) and `&n`
//! became a general Go pointer to that copy — the one value shape T24 says
//! Hybrid does not have, and one whose writes never reach the map slot.
//!
//! A binding is a slot loan when it was defined or re-assigned from
//! `&base[index]` (the same `addr_of_index_base_path` shape that creates
//! the A3 loan). The flag lives on the `Binding`, so a shadowing
//! redefinition in an inner block starts clean (no false rejects).

use std::collections::HashMap;

use crate::model::{unwrap_parens, Checker, FnState, TypeShape};
use crate::walk::{self, Use};
use hy_diag::{Diagnostic, FixIt};
use hy_syntax::{
    AssignOp, Block, CallArg, Expr, FnDecl, FnResult, ForKind, LocalDecl, LocalKind, Param, Place,
    PlaceKind, SelectArmKind, Stmt,
};

impl<'a> Checker<'a> {
    /// After `process_local`: flag every name bound to `&m[k]`, and propagate
    /// `const` composite origins returned by functions, methods, named-result
    /// bare `return`s, or closures (#429).
    pub(crate) fn mark_slot_loans_local(&self, st: &mut FnState, l: &LocalDecl) {
        if l.names.len() == l.values.len() {
            for (name, val) in l.names.iter().zip(l.values.iter()) {
                if name == "_" {
                    st.aliases.remove("_");
                    st.share_clear("_");
                    continue;
                }
                if walk::addr_of_index_base_path(val).is_some() {
                    if let Some(b) = st.env_lookup_mut(name) {
                        b.slot_loan = true;
                    }
                }
                if let Expr::FnLit {
                    params,
                    results,
                    body,
                    ..
                } = unwrap_parens(val)
                {
                    if let Some((const_name, _)) = self.fn_parts_returned_const_root(
                        params,
                        results,
                        body,
                        Some(st),
                        &[],
                        0,
                        0,
                    ) {
                        st.aliases.insert(name.clone(), const_name.clone());
                        st.share_edge(name, &const_name);
                    }
                } else if let Some((const_name, origin)) =
                    self.expr_returned_const_root(Some(st), val, 0, 0)
                {
                    self.apply_returned_const_to_binding(st, name, &const_name, origin);
                }
            }
        } else if l.values.len() == 1 && l.names.len() > 1 {
            for (i, name) in l.names.iter().enumerate() {
                if name == "_" {
                    st.aliases.remove("_");
                    st.share_clear("_");
                    continue;
                }
                if let Some((const_name, origin)) =
                    self.expr_returned_const_root(Some(st), &l.values[0], i, 0)
                {
                    self.apply_returned_const_to_binding(st, name, &const_name, origin);
                }
            }
        }
        st.aliases.remove("_");
        st.share_clear("_");
    }

    /// After `process_assign`: `x = &m[k]` makes `x` a slot loan; any other
    /// plain re-assignment of a whole binding clears the flag. Also propagates
    /// `const` composite origins returned by calls (#429).
    pub(crate) fn mark_slot_loans_assign(
        &self,
        st: &mut FnState,
        targets: &[Place],
        op: AssignOp,
        values: &[hy_syntax::Expr],
    ) {
        if op != AssignOp::Assign {
            return;
        }
        if targets.len() == values.len() {
            for (t, v) in targets.iter().zip(values.iter()) {
                if let PlaceKind::Ident(name) = &t.kind {
                    if name == "_" {
                        st.aliases.remove("_");
                        st.share_clear("_");
                        continue;
                    }
                    let is_loan = walk::addr_of_index_base_path(v).is_some();
                    if let Some(b) = st.env_lookup_mut(name) {
                        b.slot_loan = is_loan;
                    }
                    if let Expr::FnLit {
                        params,
                        results,
                        body,
                        ..
                    } = unwrap_parens(v)
                    {
                        if let Some((const_name, _)) = self.fn_parts_returned_const_root(
                            params,
                            results,
                            body,
                            Some(st),
                            &[],
                            0,
                            0,
                        ) {
                            st.aliases.insert(name.clone(), const_name.clone());
                            st.share_edge(name, &const_name);
                        }
                    } else if let Some((const_name, origin)) =
                        self.expr_returned_const_root(Some(st), v, 0, 0)
                    {
                        self.apply_returned_const_to_binding(st, name, &const_name, origin);
                    }
                }
            }
        } else if values.len() == 1 && targets.len() > 1 {
            for (i, t) in targets.iter().enumerate() {
                if let PlaceKind::Ident(name) = &t.kind {
                    if name == "_" {
                        st.aliases.remove("_");
                        st.share_clear("_");
                        continue;
                    }
                    if let Some((const_name, origin)) =
                        self.expr_returned_const_root(Some(st), &values[0], i, 0)
                    {
                        self.apply_returned_const_to_binding(st, name, &const_name, origin);
                    }
                }
            }
        }
        st.aliases.remove("_");
        st.share_clear("_");
    }

    fn apply_returned_const_to_binding(
        &self,
        st: &mut FnState,
        name: &str,
        const_name: &str,
        origin: String,
    ) {
        let Some(shape) = st.env_lookup_mut(name).map(|b| b.shape.clone()) else {
            return;
        };
        if !self.shape_may_share(&shape) && !matches!(shape, TypeShape::Unknown) {
            return;
        }
        if let Some(b) = st.env_lookup_mut(name) {
            if b.non_owned_origin.is_none() {
                b.non_owned_origin = Some(origin);
            }
        }
        if shape.is_reference_alias() || shape.is_handle() {
            st.aliases.insert(name.to_string(), const_name.to_string());
        }
        st.share_edge(name, const_name);
    }

    /// Resolve `expr` (at return slot `result_idx`) to `Some((const_name, origin))`
    /// when `expr` evaluates to reference-bearing storage backed by a `const`
    /// composite via a function, method, or closure call (or projections/slices/
    /// composite literals around one) (#429).
    pub(crate) fn expr_returned_const_root(
        &self,
        st: Option<&FnState>,
        expr: &Expr,
        result_idx: usize,
        depth: usize,
    ) -> Option<(String, String)> {
        if depth > 6 {
            return None;
        }
        let inner = unwrap_parens(expr);
        if !matches!(inner, Expr::Call { .. }) && result_idx > 0 {
            return None;
        }
        match inner {
            Expr::Try { operand, .. } | Expr::Move { operand, .. } => {
                self.expr_returned_const_root(st, operand, 0, depth + 1)
            }
            Expr::Slice { base, .. } => self.expr_returned_const_root(st, base, 0, depth + 1),
            Expr::Field { base, .. } | Expr::Index { base, .. } => {
                let empty = FnState::default();
                let eval_st = st.unwrap_or(&empty);
                let sh = self.shape_of_expr(eval_st, inner);
                if !self.shape_may_share(&sh) && !matches!(sh, TypeShape::Unknown) {
                    return None;
                }
                self.expr_returned_const_root(st, base, 0, depth + 1)
            }
            Expr::StructLit { fields, .. } => fields
                .iter()
                .find_map(|f| self.expr_returned_const_root(st, &f.value, 0, depth + 1)),
            Expr::SliceLit { items, .. } => items
                .iter()
                .find_map(|it| self.expr_returned_const_root(st, it, 0, depth + 1)),
            Expr::MapLit { entries, .. } => entries.iter().find_map(|(k, v)| {
                self.expr_returned_const_root(st, k, 0, depth + 1)
                    .or_else(|| self.expr_returned_const_root(st, v, 0, depth + 1))
            }),
            Expr::Call { callee, args, .. } => {
                let callee_inner = unwrap_parens(callee);
                match callee_inner {
                    Expr::Ident { name, .. } => {
                        let base_name = name.split('[').next().unwrap_or(name.as_str()).trim();
                        if let Some(decl) = self.fns.get(base_name) {
                            if let Some(hit) = self.fn_decl_returned_const_root(
                                decl,
                                st,
                                args,
                                result_idx,
                                depth + 1,
                            ) {
                                return Some(hit);
                            }
                        }
                        if let Some(hit) = self.local_closure_returned_const_root(
                            st,
                            base_name,
                            args,
                            result_idx,
                            depth + 1,
                        ) {
                            return Some(hit);
                        }
                    }
                    Expr::Field {
                        base,
                        name: method_name,
                        ..
                    } => {
                        let base_method = method_name
                            .split('[')
                            .next()
                            .unwrap_or(method_name.as_str())
                            .trim();
                        let empty = FnState::default();
                        let eval_st = st.unwrap_or(&empty);
                        let recv_struct = match self.shape_of_expr(eval_st, base) {
                            TypeShape::Struct(Some(s)) => Some(s),
                            TypeShape::Pointer(inner) => match *inner {
                                TypeShape::Struct(Some(s)) => Some(s),
                                _ => None,
                            },
                            _ => None,
                        };
                        if let Some(rname) = recv_struct {
                            if let Some(decl) = self
                                .method_decls
                                .get(&(rname, base_method.to_string()))
                                .copied()
                            {
                                if let Some(hit) = self.method_decl_returned_const_root(
                                    decl,
                                    st,
                                    Some(base),
                                    args,
                                    result_idx,
                                    depth + 1,
                                ) {
                                    return Some(hit);
                                }
                            }
                        }
                        for ((_, mname), decl) in &self.method_decls {
                            if mname == base_method {
                                if let Some(hit) = self.method_decl_returned_const_root(
                                    decl,
                                    st,
                                    Some(base),
                                    args,
                                    result_idx,
                                    depth + 1,
                                ) {
                                    return Some(hit);
                                }
                            }
                        }
                    }
                    Expr::FnLit {
                        params,
                        results,
                        body,
                        ..
                    } => {
                        if let Some(hit) = self.fn_parts_returned_const_root(
                            params,
                            results,
                            body,
                            st,
                            args,
                            result_idx,
                            depth + 1,
                        ) {
                            return Some(hit);
                        }
                    }
                    _ => {}
                }
                // Fallback: if an argument itself was a const-returning call
                // passed into a reference-returning helper (e.g. `same(get_shared())`).
                let empty = FnState::default();
                let eval_st = st.unwrap_or(&empty);
                let call_shape = if result_idx == 0 {
                    self.shape_of_expr(eval_st, inner)
                } else {
                    self.multi_value_shapes(eval_st, inner, result_idx + 1)
                        .into_iter()
                        .nth(result_idx)
                        .unwrap_or(TypeShape::Unknown)
                };
                if self.shape_may_share(&call_shape) || matches!(call_shape, TypeShape::Unknown) {
                    for a in args {
                        if let Some(hit) =
                            self.expr_returned_const_root(st, &a.value, 0, depth + 1)
                        {
                            return Some(hit);
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    pub(crate) fn expr_direct_or_returned_const_root(
        &self,
        st: Option<&FnState>,
        expr: &Expr,
        result_idx: usize,
        depth: usize,
    ) -> Option<(String, String)> {
        if let Some(hit) = self.expr_returned_const_root(st, expr, result_idx, depth) {
            return Some(hit);
        }
        if result_idx > 0 {
            return None;
        }
        let inner = unwrap_parens(expr);
        let empty = FnState::default();
        let eval_st = st.unwrap_or(&empty);
        let expr_shape = self.shape_of_expr(eval_st, inner);
        if !self.shape_may_share(&expr_shape) && !matches!(expr_shape, TypeShape::Unknown) {
            return None;
        }
        let mut roots = Vec::new();
        crate::model::collect_ref_roots(inner, &mut roots);
        if roots.is_empty() {
            if let Some(r) = inner.root_ident() {
                roots.push(r.to_string());
            }
        }
        for root in roots {
            if let Some(cst) = st {
                if let Some(b) = self.lookup_binding(cst, &root) {
                    if b.immutable
                        && (self.shape_may_share(&b.shape)
                            || matches!(b.shape, TypeShape::Unknown))
                    {
                        let orig = b
                            .non_owned_origin
                            .clone()
                            .unwrap_or_else(|| format!("`const {}`", root));
                        return Some((root, orig));
                    }
                    if let Some(orig) = &b.non_owned_origin {
                        if let Some(idx) = orig.find("`const ") {
                            let rest = &orig[idx + 7..];
                            if let Some(end) = rest.find('`') {
                                return Some((rest[..end].to_string(), orig.clone()));
                            }
                        }
                    }
                }
            }
            if let Some(cb) = self.consts.get(&root) {
                if self.shape_may_share(&cb.shape) || matches!(cb.shape, TypeShape::Unknown) {
                    let orig = cb
                        .non_owned_origin
                        .clone()
                        .unwrap_or_else(|| format!("file-level `const {}`", root));
                    return Some((root, orig));
                }
            }
        }
        None
    }

    fn fn_decl_returned_const_root(
        &self,
        decl: &FnDecl,
        caller_st: Option<&FnState>,
        call_args: &[CallArg],
        result_idx: usize,
        depth: usize,
    ) -> Option<(String, String)> {
        let body = decl.body.as_ref()?;
        self.fn_parts_returned_const_root(
            &decl.params,
            &decl.results,
            body,
            caller_st,
            call_args,
            result_idx,
            depth,
        )
    }

    fn method_decl_returned_const_root(
        &self,
        decl: &FnDecl,
        caller_st: Option<&FnState>,
        recv_expr: Option<&Expr>,
        call_args: &[CallArg],
        result_idx: usize,
        depth: usize,
    ) -> Option<(String, String)> {
        if depth > 6 {
            return None;
        }
        let body = decl.body.as_ref()?;
        if let Some(res_param) = decl.results.get(result_idx) {
            let ret_shape = self.shape_of_type(&res_param.ty);
            if !self.shape_may_share(&ret_shape) && !matches!(ret_shape, TypeShape::Unknown) {
                return None;
            }
        }
        let mut env_consts = self.seed_const_env(caller_st, &decl.params, call_args, depth);
        if let (Some(r), Some(rexpr)) = (&decl.receiver, recv_expr) {
            let r_shape = self.shape_of_type(&r.ty);
            if self.shape_may_share(&r_shape) || matches!(r_shape, TypeShape::Unknown) {
                if let Some(hit) =
                    self.expr_direct_or_returned_const_root(caller_st, rexpr, 0, depth + 1)
                {
                    env_consts.insert(r.name.clone(), hit);
                } else {
                    env_consts.remove(&r.name);
                }
            } else {
                env_consts.remove(&r.name);
            }
        }
        self.scan_block_for_const_return(body, &decl.results, &mut env_consts, result_idx, depth)
    }

    fn fn_parts_returned_const_root(
        &self,
        params: &[Param],
        results: &[FnResult],
        body: &Block,
        caller_st: Option<&FnState>,
        call_args: &[CallArg],
        result_idx: usize,
        depth: usize,
    ) -> Option<(String, String)> {
        if depth > 6 {
            return None;
        }
        if let Some(res_param) = results.get(result_idx) {
            let ret_shape = self.shape_of_type(&res_param.ty);
            if !self.shape_may_share(&ret_shape) && !matches!(ret_shape, TypeShape::Unknown) {
                return None;
            }
        }
        let mut env_consts = self.seed_const_env(caller_st, params, call_args, depth);
        self.scan_block_for_const_return(body, results, &mut env_consts, result_idx, depth)
    }

    fn seed_const_env(
        &self,
        caller_st: Option<&FnState>,
        params: &[Param],
        call_args: &[CallArg],
        depth: usize,
    ) -> HashMap<String, (String, String)> {
        let mut env_consts: HashMap<String, (String, String)> = HashMap::new();
        for (cname, cb) in &self.consts {
            if self.shape_may_share(&cb.shape) || matches!(cb.shape, TypeShape::Unknown) {
                let origin = cb
                    .non_owned_origin
                    .clone()
                    .unwrap_or_else(|| format!("file-level `const {}`", cname));
                env_consts.insert(cname.clone(), (cname.clone(), origin));
            }
        }
        if let Some(cst) = caller_st {
            for (bname, b) in cst
                .env
                .iter()
                .chain(cst.scopes.iter().flat_map(|s| s.iter()))
            {
                if b.immutable
                    && (self.shape_may_share(&b.shape) || matches!(b.shape, TypeShape::Unknown))
                {
                    let origin = b
                        .non_owned_origin
                        .clone()
                        .unwrap_or_else(|| format!("`const {}`", bname));
                    env_consts.insert(bname.clone(), (bname.clone(), origin));
                } else if let Some(orig) = &b.non_owned_origin {
                    if let Some(idx) = orig.find("`const ") {
                        let rest = &orig[idx + 7..];
                        if let Some(end) = rest.find('`') {
                            let cname = rest[..end].to_string();
                            env_consts.insert(bname.clone(), (cname, orig.clone()));
                        }
                    }
                }
            }
        }
        for (i, p) in params.iter().enumerate() {
            let p_shape = self.shape_of_type(&p.ty);
            if self.shape_may_share(&p_shape) || matches!(p_shape, TypeShape::Unknown) {
                if let Some(arg) = call_args.get(i) {
                    if let Some(hit) = self.expr_direct_or_returned_const_root(
                        caller_st,
                        &arg.value,
                        0,
                        depth + 1,
                    ) {
                        env_consts.insert(p.name.clone(), hit);
                        continue;
                    }
                }
            }
            env_consts.remove(&p.name);
        }
        env_consts
    }

    fn scan_block_for_const_return(
        &self,
        block: &Block,
        results: &[FnResult],
        env_consts: &mut HashMap<String, (String, String)>,
        result_idx: usize,
        depth: usize,
    ) -> Option<(String, String)> {
        for s in &block.stmts {
            if let Some(hit) =
                self.scan_stmt_for_const_return(s, results, env_consts, result_idx, depth)
            {
                return Some(hit);
            }
        }
        None
    }

    fn scan_stmt_for_const_return(
        &self,
        stmt: &Stmt,
        results: &[FnResult],
        env_consts: &mut HashMap<String, (String, String)>,
        result_idx: usize,
        depth: usize,
    ) -> Option<(String, String)> {
        match stmt {
            Stmt::Local(l) => {
                if l.kind == LocalKind::Const {
                    let empty = FnState::default();
                    for (name, val) in l.names.iter().zip(l.values.iter()) {
                        let sh = l
                            .ty
                            .as_ref()
                            .map(|t| self.shape_of_type(t))
                            .unwrap_or_else(|| self.shape_of_expr(&empty, val));
                        if self.shape_may_share(&sh) || matches!(sh, TypeShape::Unknown) {
                            env_consts
                                .insert(name.clone(), (name.clone(), format!("`const {}`", name)));
                        }
                    }
                } else if l.values.is_empty() {
                    for name in &l.names {
                        env_consts.remove(name);
                    }
                } else if l.names.len() == l.values.len() {
                    let mut updates = Vec::new();
                    for (name, val) in l.names.iter().zip(l.values.iter()) {
                        updates.push((
                            name.clone(),
                            self.expr_in_fn_const_root(val, 0, env_consts, depth + 1),
                        ));
                    }
                    for (name, hit) in updates {
                        if let Some(h) = hit {
                            env_consts.insert(name, h);
                        } else {
                            env_consts.remove(&name);
                        }
                    }
                } else if l.values.len() == 1 && l.names.len() > 1 {
                    let mut updates = Vec::new();
                    for (i, name) in l.names.iter().enumerate() {
                        updates.push((
                            name.clone(),
                            self.expr_in_fn_const_root(&l.values[0], i, env_consts, depth + 1),
                        ));
                    }
                    for (name, hit) in updates {
                        if let Some(h) = hit {
                            env_consts.insert(name, h);
                        } else {
                            env_consts.remove(&name);
                        }
                    }
                }
            }
            Stmt::Assign {
                targets,
                op: AssignOp::Assign,
                values,
                ..
            } => {
                if targets.len() == values.len() {
                    let mut updates = Vec::new();
                    for (t, v) in targets.iter().zip(values.iter()) {
                        if let PlaceKind::Ident(name) = &t.kind {
                            updates.push((
                                name.clone(),
                                self.expr_in_fn_const_root(v, 0, env_consts, depth + 1),
                            ));
                        }
                    }
                    for (name, hit) in updates {
                        if let Some(h) = hit {
                            env_consts.insert(name, h);
                        } else {
                            env_consts.remove(&name);
                        }
                    }
                } else if values.len() == 1 && targets.len() > 1 {
                    let mut updates = Vec::new();
                    for (i, t) in targets.iter().enumerate() {
                        if let PlaceKind::Ident(name) = &t.kind {
                            updates.push((
                                name.clone(),
                                self.expr_in_fn_const_root(&values[0], i, env_consts, depth + 1),
                            ));
                        }
                    }
                    for (name, hit) in updates {
                        if let Some(h) = hit {
                            env_consts.insert(name, h);
                        } else {
                            env_consts.remove(&name);
                        }
                    }
                }
            }
            Stmt::Return { values, .. } => {
                if values.is_empty() {
                    if let Some(rname) = results.get(result_idx).and_then(|r| r.name.as_ref()) {
                        if let Some(hit) = env_consts.get(rname) {
                            return Some(hit.clone());
                        }
                    }
                } else if let Some(v) = values.get(result_idx) {
                    if let Some(hit) = self.expr_in_fn_const_root(v, 0, env_consts, depth + 1) {
                        return Some(hit);
                    }
                } else if values.len() == 1 {
                    if let Some(hit) =
                        self.expr_in_fn_const_root(&values[0], result_idx, env_consts, depth + 1)
                    {
                        return Some(hit);
                    }
                }
            }
            Stmt::If {
                init,
                then,
                otherwise,
                ..
            } => {
                let mut branch_env = env_consts.clone();
                if let Some(i) = init {
                    if let Some(hit) =
                        self.scan_stmt_for_const_return(i, results, &mut branch_env, result_idx, depth)
                    {
                        return Some(hit);
                    }
                }
                let mut then_env = branch_env.clone();
                if let Some(hit) =
                    self.scan_block_for_const_return(then, results, &mut then_env, result_idx, depth)
                {
                    return Some(hit);
                }
                if let Some(o) = otherwise {
                    let mut else_env = branch_env;
                    if let Some(hit) = self
                        .scan_stmt_for_const_return(o, results, &mut else_env, result_idx, depth)
                    {
                        return Some(hit);
                    }
                }
            }
            Stmt::For { kind, body, .. } => {
                let mut loop_env = env_consts.clone();
                if let ForKind::Iterate { vars, expr } = kind {
                    if let Some(hit) = self.expr_in_fn_const_root(expr, 0, env_consts, depth + 1) {
                        if let Some(last_v) = vars.last() {
                            if last_v != "_" {
                                loop_env.insert(last_v.clone(), hit);
                            }
                        }
                    }
                }
                if let Some(hit) =
                    self.scan_block_for_const_return(body, results, &mut loop_env, result_idx, depth)
                {
                    return Some(hit);
                }
            }
            Stmt::Managed { body, .. } | Stmt::Block { block: body, .. } => {
                let mut inner_env = env_consts.clone();
                if let Some(hit) = self
                    .scan_block_for_const_return(body, results, &mut inner_env, result_idx, depth)
                {
                    return Some(hit);
                }
            }
            Stmt::Match { arms, .. } => {
                for arm in arms {
                    let mut arm_env = env_consts.clone();
                    if let Some(hit) = self.scan_block_for_const_return(
                        &arm.body,
                        results,
                        &mut arm_env,
                        result_idx,
                        depth,
                    ) {
                        return Some(hit);
                    }
                }
            }
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    let mut arm_env = env_consts.clone();
                    if let SelectArmKind::Recv { names, chan } = &arm.kind {
                        if let Some(first) = names.first() {
                            if let Some(hit) =
                                self.expr_in_fn_const_root(chan, 0, env_consts, depth + 1)
                            {
                                arm_env.insert(first.clone(), hit);
                            }
                        }
                    }
                    if let Some(hit) = self.scan_block_for_const_return(
                        &arm.body,
                        results,
                        &mut arm_env,
                        result_idx,
                        depth,
                    ) {
                        return Some(hit);
                    }
                }
            }
            _ => {}
        }
        None
    }

    fn expr_in_fn_const_root(
        &self,
        expr: &Expr,
        slot_idx: usize,
        env_consts: &HashMap<String, (String, String)>,
        depth: usize,
    ) -> Option<(String, String)> {
        if depth > 6 {
            return None;
        }
        let inner = unwrap_parens(expr);
        if !matches!(inner, Expr::Call { .. }) && slot_idx > 0 {
            return None;
        }
        match inner {
            Expr::Ident { name, .. } => env_consts.get(name).cloned(),
            Expr::Try { operand, .. } | Expr::Move { operand, .. } => {
                self.expr_in_fn_const_root(operand, 0, env_consts, depth + 1)
            }
            Expr::Slice { base, .. } => self.expr_in_fn_const_root(base, 0, env_consts, depth + 1),
            Expr::Field { base, .. } | Expr::Index { base, .. } => {
                let empty = FnState::default();
                let sh = self.shape_of_expr(&empty, inner);
                if !self.shape_may_share(&sh) && !matches!(sh, TypeShape::Unknown) {
                    return None;
                }
                self.expr_in_fn_const_root(base, 0, env_consts, depth + 1)
            }
            Expr::StructLit { fields, .. } => fields
                .iter()
                .find_map(|f| self.expr_in_fn_const_root(&f.value, 0, env_consts, depth + 1)),
            Expr::SliceLit { items, .. } => items
                .iter()
                .find_map(|it| self.expr_in_fn_const_root(it, 0, env_consts, depth + 1)),
            Expr::MapLit { entries, .. } => entries.iter().find_map(|(k, v)| {
                self.expr_in_fn_const_root(k, 0, env_consts, depth + 1)
                    .or_else(|| self.expr_in_fn_const_root(v, 0, env_consts, depth + 1))
            }),
            Expr::FnLit {
                params,
                results,
                body,
                ..
            } => {
                let mut inner_env = env_consts.clone();
                for p in params {
                    inner_env.remove(&p.name);
                }
                self.scan_block_for_const_return(body, results, &mut inner_env, 0, depth + 1)
            }
            Expr::Call { .. } => {
                if let Some(hit) = self.expr_returned_const_root(None, inner, slot_idx, depth + 1) {
                    return Some(hit);
                }
                let Expr::Call { args, .. } = inner else {
                    return None;
                };
                for a in args {
                    if let Some(hit) =
                        self.expr_in_fn_const_root(&a.value, 0, env_consts, depth + 1)
                    {
                        return Some(hit);
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn local_closure_returned_const_root(
        &self,
        st: Option<&FnState>,
        name: &str,
        args: &[CallArg],
        result_idx: usize,
        depth: usize,
    ) -> Option<(String, String)> {
        if depth > 6 {
            return None;
        }
        if let Some(cst) = st {
            if let Some(target) = cst.aliases.get(name) {
                if let Some(cb) = self.consts.get(target) {
                    if self.shape_may_share(&cb.shape) || matches!(cb.shape, TypeShape::Unknown) {
                        let orig = cb
                            .non_owned_origin
                            .clone()
                            .unwrap_or_else(|| format!("file-level `const {}`", target));
                        return Some((target.clone(), orig));
                    }
                }
                if let Some(b) = self.lookup_binding(cst, target) {
                    if b.immutable {
                        let orig = b
                            .non_owned_origin
                            .clone()
                            .unwrap_or_else(|| format!("`const {}`", target));
                        return Some((target.clone(), orig));
                    }
                }
            }
        }
        if let Some((params, results, body)) = self.find_local_closure_parts_in_file(name) {
            return self.fn_parts_returned_const_root(
                params,
                results,
                body,
                st,
                args,
                result_idx,
                depth + 1,
            );
        }
        None
    }

    pub(crate) fn find_local_closure_parts_in_file(
        &self,
        target_name: &str,
    ) -> Option<(&'a [Param], &'a [FnResult], &'a Block)> {
        for f in self.file.fns() {
            if let Some(body) = &f.body {
                if let Some(found) = Self::find_closure_in_block(body, target_name) {
                    return Some(found);
                }
            }
        }
        None
    }

    fn find_closure_in_block<'b>(
        block: &'b Block,
        target_name: &str,
    ) -> Option<(&'b [Param], &'b [FnResult], &'b Block)> {
        for s in &block.stmts {
            if let Some(found) = Self::find_closure_in_stmt(s, target_name) {
                return Some(found);
            }
        }
        None
    }

    fn find_closure_in_stmt<'b>(
        stmt: &'b Stmt,
        target_name: &str,
    ) -> Option<(&'b [Param], &'b [FnResult], &'b Block)> {
        match stmt {
            Stmt::Local(l) => {
                for (n, v) in l.names.iter().zip(l.values.iter()) {
                    if n == target_name {
                        if let Expr::FnLit {
                            params,
                            results,
                            body,
                            ..
                        } = unwrap_parens(v)
                        {
                            return Some((params, results, body));
                        }
                    }
                }
            }
            Stmt::Assign {
                targets, values, ..
            } => {
                for (t, v) in targets.iter().zip(values.iter()) {
                    if let PlaceKind::Ident(n) = &t.kind {
                        if n == target_name {
                            if let Expr::FnLit {
                                params,
                                results,
                                body,
                                ..
                            } = unwrap_parens(v)
                            {
                                return Some((params, results, body));
                            }
                        }
                    }
                }
            }
            Stmt::If {
                init,
                then,
                otherwise,
                ..
            } => {
                if let Some(i) = init {
                    if let Some(f) = Self::find_closure_in_stmt(i, target_name) {
                        return Some(f);
                    }
                }
                if let Some(f) = Self::find_closure_in_block(then, target_name) {
                    return Some(f);
                }
                if let Some(o) = otherwise {
                    if let Some(f) = Self::find_closure_in_stmt(o, target_name) {
                        return Some(f);
                    }
                }
            }
            Stmt::For { body, .. }
            | Stmt::Managed { body, .. }
            | Stmt::Block { block: body, .. } => {
                if let Some(f) = Self::find_closure_in_block(body, target_name) {
                    return Some(f);
                }
            }
            Stmt::Match { arms, .. } => {
                for arm in arms {
                    if let Some(f) = Self::find_closure_in_block(&arm.body, target_name) {
                        return Some(f);
                    }
                }
            }
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    if let Some(f) = Self::find_closure_in_block(&arm.body, target_name) {
                        return Some(f);
                    }
                }
            }
            _ => {}
        }
        None
    }

    /// T24: `&x` where `x` is a live slot-loan binding.
    pub(crate) fn check_addr_of_loan(&mut self, st: &FnState, uses: &[Use]) {
        for u in uses.iter().filter(|u| u.via_addr) {
            let Some(b) = self.lookup_binding(st, &u.name) else {
                continue;
            };
            if !b.slot_loan {
                continue;
            }
            let msg = format!(
                "`&{}` takes the address of a loan: a map-slot loan is not an addressable value (T24)",
                u.name
            );
            let mut d = Diagnostic::error("HYB-E3104", self.path.clone(), u.span, msg)
                .with_notes(vec![format!(
                    "`{}` is already the only general-value `&` Hybrid has (an O8 map-slot loan); a pointer to it would be a general pointer, which Tier 0 does not have",
                    u.name
                )]);
            // The registry fix-its for E3104 describe the O2 return-escape
            // shape; replace them with the T24 one for this site.
            d.fixits = vec![FixIt {
                description: format!(
                    "use `{}` directly, or take a fresh loan of the slot (`&m[k]`) where it is needed",
                    u.name
                ),
                replacement: None,
            }];
            self.diags.push(d);
        }
    }
}
