//! a2_interproc.rs — interprocedural, closure, method, and indirect-call
//! A2 (`O7` + `O11`, `HYB-E3101`) boundary-handle borrow-loan invalidation.

use crate::foreign::{self, CallMode};
use crate::loans::{a3_paths_conflict, A3_PURE_FNS, A3_PURE_PKGS};
use crate::model::{unwrap_parens, Checker, FnState, LoanTarget, TypeShape};
use crate::shape::receiver_type_name;
use crate::walk::{self, CallRef};
use hy_syntax::{Block, Expr, FnDecl, ForKind, Param, PlaceKind, Stmt};
use std::collections::{HashMap, HashSet};

type ClosureDef = (Vec<Param>, Block);
const PURE_METHODS: &[&str] = &["unwrap", "or", "is_some", "is_none", "insert", "contains", "remove", "len", "pop"];
const SCALAR_FNS: &[&str] = &[
    "len", "cap", "min", "max", "int", "int8", "int16", "int32", "int64", "uint", "uint8",
    "uint16", "uint32", "uint64", "uintptr", "float32", "float64", "bool", "byte", "rune", "string",
];

fn struct_base_of(shape: &TypeShape) -> Option<String> {
    match shape {
        TypeShape::Struct(Some(s)) => Some(Checker::base_named_type_str(s).to_string()),
        TypeShape::Pointer(inner) => struct_base_of(inner),
        _ => None,
    }
}

fn is_pure_builtin_fn(name: &str, head: &str) -> bool {
    A3_PURE_FNS.contains(&name) || matches!(head, "some" | "delete" | "clear" | "min" | "max")
}

impl<'a> Checker<'a> {
    fn live_handle_loans(&self, st: &FnState, idx: usize) -> Vec<String> {
        st.loans.iter().filter_map(|l| match &l.target {
            LoanTarget::Handle { handle } if l.created <= idx && idx <= self.effective_loan_last(st, l) => Some(handle.clone()),
            _ => None,
        }).collect()
    }

    fn candidate_roots_for_loans(&self, st: &FnState, live: &[String]) -> Vec<String> {
        let mut names: HashSet<String> = live.iter().cloned().collect();
        for k in st.env.keys().chain(st.scopes.iter().flat_map(|s| s.keys())) {
            if live.iter().any(|lh| a3_paths_conflict(st, lh, k)) { names.insert(k.clone()); }
        }
        names.into_iter().collect()
    }

    fn is_non_handle_shape(&self, sh: &TypeShape) -> bool {
        !matches!(sh, TypeShape::Unknown | TypeShape::Closure) && !self.shape_carries_handle(sh)
    }

    pub(crate) fn collect_handle_expr_roots(&self, st_opt: Option<&FnState>, e: &Expr, out: &mut Vec<String>) {
        match unwrap_parens(e) {
            Expr::Ident { name, .. } if name != "_" => out.push(name.clone()),
            Expr::Move { operand, .. } | Expr::Try { operand, .. } | Expr::AddrOf { operand, .. }
            | Expr::Unary { op: hy_syntax::UnOp::Addr, operand, .. }
            | Expr::Slice { base: operand, .. } | Expr::Index { base: operand, .. } => self.collect_handle_expr_roots(st_opt, operand, out),
            Expr::Field { base, .. } => {
                if !st_opt.is_some_and(|st| self.is_non_handle_shape(&self.shape_of_expr(st, e))) {
                    self.collect_handle_expr_roots(st_opt, base, out);
                }
            }
            Expr::StructLit { fields, .. } => fields.iter().for_each(|f| self.collect_handle_expr_roots(st_opt, &f.value, out)),
            Expr::SliceLit { items, .. } => items.iter().for_each(|it| self.collect_handle_expr_roots(st_opt, it, out)),
            Expr::MapLit { entries, .. } => entries.iter().for_each(|(k, v)| { self.collect_handle_expr_roots(st_opt, k, out); self.collect_handle_expr_roots(st_opt, v, out); }),
            Expr::Call { callee, args, .. } => {
                let stop = st_opt.map_or_else(|| self.call_returns_non_handle_without_state(callee), |st| self.is_non_handle_shape(&self.shape_of_expr(st, e)));
                if !stop {
                    args.iter().for_each(|a| self.collect_handle_expr_roots(st_opt, &a.value, out));
                    self.collect_handle_expr_roots(st_opt, callee, out);
                }
            }
            _ => {}
        }
    }

    fn call_returns_non_handle_without_state(&self, callee: &Expr) -> bool {
        let non_handle_decl = |f: &FnDecl| f.results.first().is_some_and(|r| self.is_non_handle_shape(&self.shape_of_type(&r.ty)));
        match unwrap_parens(callee) {
            Expr::Ident { name, .. } => {
                let head = Self::base_named_type_str(name);
                SCALAR_FNS.contains(&head) || self.fns.get(name.as_str()).or_else(|| self.fns.get(head)).is_some_and(|f| non_handle_decl(f))
            }
            Expr::Field { base, name, .. } => {
                let head = Self::base_named_type_str(name);
                if let Expr::Ident { name: pkg, .. } = unwrap_parens(base) {
                    if self.known_package(pkg) {
                        let sh = foreign::package_result(pkg, name);
                        return !matches!(sh, TypeShape::Unknown) && !self.shape_carries_handle(&sh);
                    }
                    if let Some(f) = self.fns.get(format!("{pkg}.{head}").as_str()) { return non_handle_decl(f); }
                }
                !self.method_decls.iter().any(|((_, m), _)| m == head) && matches!(head, "int" | "string" | "len" | "cap" | "is_some" | "is_none" | "contains")
            }
            _ => false,
        }
    }

    pub(crate) fn check_a2_package_call(&mut self, st: &FnState, cr: &CallRef, idx: usize, pkg: &str, method: &str) {
        let live = self.live_handle_loans(st, idx);
        if live.is_empty() { return; }
        if foreign::package_mode(pkg, method) == CallMode::Mutate { self.check_a2_unknown_args(st, cr, idx, &live); }
        self.check_a2_closure_args(st, cr, idx, &live, None);
    }

    pub(crate) fn check_a2_method_call(&mut self, st: &FnState, cr: &CallRef, idx: usize, root: &str, recv: &TypeShape, method: &str) {
        let live = self.live_handle_loans(st, idx);
        if live.is_empty() { return; }
        let head = Self::base_named_type_str(method);
        let Expr::Field { base, .. } = unwrap_parens(cr.callee) else { return };
        if let Expr::Ident { name: base_id, .. } = unwrap_parens(base) {
            if self.lookup_binding(st, base_id).is_none() && !st.type_params.contains(base_id.as_str()) {
                if let Some(&decl) = self.fns.get(format!("{base_id}.{head}").as_str()) {
                    return self.check_a2_decl_and_closures(st, cr, idx, &live, decl, None);
                }
            }
        }
        if let Some(rb) = self.ty_of_expr(st, base).map(|t| self.resolve_type_alias(&t)).and_then(|t| Self::base_named_type(&t)).or_else(|| struct_base_of(recv)) {
            if let Some(&decl) = self.method_decls.get(&(rb, head.to_string())) {
                return self.check_a2_decl_and_closures(st, cr, idx, &live, decl, Some(base));
            }
        }
        if PURE_METHODS.contains(&head) && !self.method_decls.iter().any(|((_, m), _)| m == head) { return; }
        if live.iter().any(|lh| a3_paths_conflict(st, lh, root)) { return self.check_a2_conflict(st, cr, root, idx); }
        self.check_a2_unknown_args(st, cr, idx, &live);
        self.check_a2_closure_args(st, cr, idx, &live, None);
    }

    pub(crate) fn check_a2_fn_call(&mut self, st: &FnState, cr: &CallRef, idx: usize, name: &str) {
        let live = self.live_handle_loans(st, idx);
        if live.is_empty() { return; }
        let head = Self::base_named_type_str(name);
        if self.lookup_binding(st, name).is_none() && self.lookup_binding(st, head).is_none() {
            if let Some(&decl) = self.fns.get(name).or_else(|| self.fns.get(head)) {
                return self.check_a2_decl_and_closures(st, cr, idx, &live, decl, None);
            }
            if is_pure_builtin_fn(name, head) { return; }
        }
        let resolved = crate::freeze::resolve_alias(st, name);
        if let Some((params, results, body)) = st.closure_defs.get(name).or_else(|| st.closure_defs.get(&resolved)).cloned() {
            return self.check_a2_closure_body_and_args(st, cr, idx, &live, &params, &results, &body);
        }
        self.check_a2_unknown_args(st, cr, idx, &live);
        self.check_a2_closure_args(st, cr, idx, &live, None);
    }

    pub(crate) fn check_a2_indirect_call(&mut self, st: &FnState, cr: &CallRef, idx: usize) {
        let live = self.live_handle_loans(st, idx);
        if live.is_empty() { return; }
        match unwrap_parens(cr.callee) {
            Expr::FnLit { params, results, body, .. } => return self.check_a2_closure_body_and_args(st, cr, idx, &live, params, results, body),
            Expr::Field { base, name, .. } => {
                let recv_shape = self.shape_of_expr(st, base);
                if let Some(tname) = receiver_type_name(&recv_shape) {
                    if matches!(foreign::method_mode(&tname, name), CallMode::Mutate | CallMode::Consume) {
                        let mut roots = Vec::new();
                        self.collect_handle_expr_roots(Some(st), base, &mut roots);
                        if name == "set" { if let Some(varg) = cr.args.get(1) { self.collect_handle_expr_roots(Some(st), &varg.value, &mut roots); } }
                        for r in roots { if live.iter().any(|lh| a3_paths_conflict(st, lh, &r)) { self.check_a2_conflict(st, cr, &r, idx); } }
                    }
                    return self.check_a2_closure_args(st, cr, idx, &live, None);
                }
                if let Some((sb, head)) = struct_base_of(&recv_shape).map(|sb| (sb, Self::base_named_type_str(name).to_string())) {
                    if let Some(&decl) = self.method_decls.get(&(sb, head)) {
                        return self.check_a2_decl_and_closures(st, cr, idx, &live, decl, Some(base));
                    }
                }
            }
            Expr::Index { base, .. } => if let Expr::Ident { name: gname, .. } = unwrap_parens(base) {
                if self.lookup_binding(st, gname).is_none() {
                    if let Some(&decl) = self.fns.get(gname.as_str()) { return self.check_a2_decl_and_closures(st, cr, idx, &live, decl, None); }
                }
            },
            _ => {}
        }
        self.check_a2_unknown_args(st, cr, idx, &live);
        self.check_a2_closure_args(st, cr, idx, &live, None);
    }

    pub(crate) fn check_a2_foreign_method_closure_args(&mut self, st: &FnState, cr: &CallRef, idx: usize) {
        let live = self.live_handle_loans(st, idx);
        if !live.is_empty() { self.check_a2_closure_args(st, cr, idx, &live, None); }
    }

    fn check_a2_decl_and_closures(&mut self, st: &FnState, cr: &CallRef, idx: usize, live: &[String], decl: &FnDecl, recv_expr: Option<&Expr>) {
        if let (Some(base), Some(rp)) = (recv_expr, &decl.receiver) {
            let mut rroots = Vec::new();
            self.collect_handle_expr_roots(Some(st), base, &mut rroots);
            if let Some(rr) = rroots.into_iter().find(|rr| live.iter().any(|lh| a3_paths_conflict(st, lh, rr)) && self.fn_mutates_or_consumes_handle(decl, &rp.name, 0)) {
                self.check_a2_conflict(st, cr, &rr, idx);
            }
        }
        for (i, a) in cr.args.iter().enumerate() {
            let mut roots = Vec::new();
            self.collect_handle_expr_roots(Some(st), &a.value, &mut roots);
            if let Some(r) = roots.into_iter().find(|r| live.iter().any(|lh| a3_paths_conflict(st, lh, r)) && decl.params.get(i).map(|p| self.fn_mutates_or_consumes_handle(decl, &p.name, 0)).unwrap_or(true)) {
                self.check_a2_conflict(st, cr, &r, idx);
            }
        }
        self.check_a2_closure_args(st, cr, idx, live, Some(decl));
    }

    fn check_a2_unknown_args(&mut self, st: &FnState, cr: &CallRef, idx: usize, live: &[String]) {
        for a in cr.args {
            let mut roots = Vec::new();
            self.collect_handle_expr_roots(Some(st), &a.value, &mut roots);
            for r in roots { if live.iter().any(|lh| a3_paths_conflict(st, lh, &r)) { self.check_a2_conflict(st, cr, &r, idx); } }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn check_a2_closure_body_and_args(&mut self, st: &FnState, cr: &CallRef, idx: usize, live: &[String], params: &[Param], results: &[hy_syntax::FnResult], body: &Block) {
        let (bound, shapes) = self.closure_env(Some(st), params, results);
        if let Some(cand) = self.candidate_roots_for_loans(st, live).into_iter().find(|c| !bound.contains(c) && self.block_mutates_or_consumes_handle(Some(st), &shapes, &bound, body, c, 0)) {
            self.check_a2_conflict(st, cr, &cand, idx);
        }
        for (i, a) in cr.args.iter().enumerate() {
            let mut roots = Vec::new();
            self.collect_handle_expr_roots(Some(st), &a.value, &mut roots);
            if let Some(r) = roots.into_iter().find(|r| live.iter().any(|lh| a3_paths_conflict(st, lh, r)) && params.get(i).map(|p| self.block_mutates_or_consumes_handle(Some(st), &shapes, &bound, body, &p.name, 0)).unwrap_or(true)) {
                self.check_a2_conflict(st, cr, &r, idx);
            }
        }
        self.check_a2_closure_args(st, cr, idx, live, None);
    }

    fn check_a2_closure_args(&mut self, st: &FnState, cr: &CallRef, idx: usize, live: &[String], callee_decl: Option<&FnDecl>) {
        let cands = self.candidate_roots_for_loans(st, live);
        for (i, a) in cr.args.iter().enumerate() {
            if callee_decl.is_some_and(|d| !d.params.get(i).map(|p| self.block_invokes_param(d.body.as_ref(), &p.name)).unwrap_or(true)) { continue; }
            let cls_opt = match unwrap_parens(&a.value) {
                Expr::FnLit { params, results, body, .. } => Some((params.clone(), results.clone(), body.clone())),
                Expr::Ident { name, .. } => st.closure_defs.get(name).or_else(|| st.closure_defs.get(&crate::freeze::resolve_alias(st, name))).cloned(),
                _ => None,
            };
            let Some((params, results, body)) = cls_opt else { continue };
            let (bound, shapes) = self.closure_env(Some(st), &params, &results);
            if let Some(cand) = cands.iter().find(|c| !bound.contains(*c) && self.block_mutates_or_consumes_handle(Some(st), &shapes, &bound, &body, c, 0)) {
                return self.check_a2_conflict(st, cr, cand, idx);
            }
        }
    }

    fn closure_env(&self, st_opt: Option<&FnState>, params: &[Param], results: &[hy_syntax::FnResult]) -> (HashSet<String>, HashMap<String, TypeShape>) {
        let mut bound: HashSet<String> = params.iter().map(|p| p.name.clone()).collect();
        bound.extend(results.iter().filter_map(|r| r.name.clone()));
        let mut shapes = HashMap::new();
        if let Some(st) = st_opt { for (k, b) in st.env.iter().chain(st.scopes.iter().flat_map(|s| s.iter())) { shapes.insert(k.clone(), b.shape.clone()); } }
        for p in params { shapes.insert(p.name.clone(), self.shape_of_type(&p.ty)); }
        for r in results { if let Some(rn) = &r.name { shapes.insert(rn.clone(), self.shape_of_type(&r.ty)); } }
        (bound, shapes)
    }

    fn block_invokes_param(&self, body: Option<&Block>, param: &str) -> bool {
        body.is_some_and(|b| walk::flatten(b).stmts.iter().any(|(stmt, _)| {
            let mut calls = Vec::new();
            walk::own_calls(stmt, &mut calls);
            calls.iter().any(|cr| matches!(unwrap_parens(cr.callee), Expr::Ident { name, .. } if name == param) || cr.args.iter().any(|a| a.value.root_ident() == Some(param)))
        }))
    }

    pub(crate) fn fn_mutates_or_consumes_handle(&self, decl: &FnDecl, target: &str, depth: usize) -> bool {
        if depth > 4 { return true; }
        let Some(body) = &decl.body else { return false };
        let (mut bound, mut shapes) = self.closure_env(None, &decl.params, &decl.results);
        if let Some(r) = &decl.receiver { shapes.insert(r.name.clone(), self.shape_of_type(&r.ty)); bound.insert(r.name.clone()); }
        self.block_mutates_or_consumes_handle(None, &shapes, &bound, body, target, depth)
    }

    fn block_mutates_or_consumes_handle(&self, st_opt: Option<&FnState>, init_shapes: &HashMap<String, TypeShape>, init_bound: &HashSet<String>, block: &Block, target: &str, depth: usize) -> bool {
        if depth > 4 { return true; }
        let flat = walk::flatten(block);
        let mut alias_stack: Vec<HashSet<String>> = vec![[target.to_string()].into_iter().collect()];
        let mut locals_stack: Vec<HashSet<String>> = vec![HashSet::new()];
        let (mut bound_names, mut shapes, mut local_closures) = (init_bound.clone(), init_shapes.clone(), HashMap::<String, ClosureDef>::new());
        for (stmt, idx) in &flat.stmts {
            for ev in flat.events.get(idx).into_iter().flatten() {
                match ev {
                    walk::ScopeEvent::Push => { alias_stack.push(alias_stack.last().unwrap().clone()); locals_stack.push(HashSet::new()); }
                    walk::ScopeEvent::Pop if alias_stack.len() > 1 => {
                        let (pa, pl) = (alias_stack.pop().unwrap(), locals_stack.pop().unwrap());
                        for a in pa { if !pl.contains(&a) { alias_stack.last_mut().unwrap().insert(a); } }
                    }
                    walk::ScopeEvent::MatchBind { name, subject } => {
                        bound_names.insert(name.to_string());
                        let mut sr = Vec::new();
                        self.collect_handle_expr_roots(None, subject, &mut sr);
                        if sr.iter().any(|r| alias_stack.last().unwrap().contains(r)) { alias_stack.last_mut().unwrap().insert(name.to_string()); }
                    }
                    _ => {}
                }
            }
            if let Stmt::For { kind: ForKind::Iterate { vars, expr }, .. } = stmt {
                let mut ir = Vec::new();
                self.collect_handle_expr_roots(None, expr, &mut ir);
                let shares = ir.iter().any(|r| alias_stack.last().unwrap().contains(r));
                for v in vars {
                    bound_names.insert(v.clone());
                    locals_stack.last_mut().unwrap().insert(v.clone());
                    if shares { alias_stack.last_mut().unwrap().insert(v.clone()); } else { alias_stack.last_mut().unwrap().remove(v); }
                }
            }
            let pure_fnlit = matches!(stmt, Stmt::Local(l) if l.values.iter().all(|v| matches!(unwrap_parens(v), Expr::FnLit { .. })))
                || matches!(stmt, Stmt::Assign { targets, values, .. } if targets.iter().all(|t| matches!(t.kind, PlaceKind::Ident(_))) && values.iter().all(|v| matches!(unwrap_parens(v), Expr::FnLit { .. })));
            if !pure_fnlit {
                let mut fnlits = Vec::new();
                walk::own_fnlits(stmt, &mut fnlits);
                for fl in fnlits {
                    if let Expr::FnLit { params, results, body: fbody, .. } = fl {
                        let (fbound, mut fshapes) = self.closure_env(None, params, results);
                        for (k, v) in &shapes { fshapes.entry(k.clone()).or_insert_with(|| v.clone()); }
                        if alias_stack.last().unwrap().iter().any(|p| !fbound.contains(p) && self.block_mutates_or_consumes_handle(st_opt, &fshapes, &fbound, fbody, p, depth + 1)) { return true; }
                    }
                }
            }
            let mut calls = Vec::new();
            walk::own_calls(stmt, &mut calls);
            if calls.iter().any(|cr| alias_stack.last().unwrap().iter().any(|p| self.call_mutates_or_consumes_handle(st_opt, &shapes, &bound_names, &local_closures, cr, p, depth))) { return true; }
            let pre = alias_stack.last().unwrap().clone();
            match stmt {
                Stmt::Local(l) if l.values.is_empty() => for name in &l.names {
                    bound_names.insert(name.clone());
                    locals_stack.last_mut().unwrap().insert(name.clone());
                    if let Some(t) = &l.ty { shapes.insert(name.clone(), self.shape_of_type(t)); }
                    alias_stack.last_mut().unwrap().remove(name);
                },
                Stmt::Local(l) => {
                    let vals: Vec<&Expr> = if l.values.len() == 1 { vec![&l.values[0]; l.names.len()] } else { l.values.iter().collect() };
                    for (name, val) in l.names.iter().zip(vals) {
                        bound_names.insert(name.clone());
                        locals_stack.last_mut().unwrap().insert(name.clone());
                        if let Some(sh) = l.ty.as_ref().map(|t| self.shape_of_type(t)).or_else(|| self.hint_expr_shape(&shapes, val)) { shapes.insert(name.clone(), sh); }
                        self.rebind_alias(&mut local_closures, alias_stack.last_mut().unwrap(), &pre, name, val);
                    }
                }
                Stmt::Assign { targets, values, .. } if targets.len() == values.len() => {
                    for (t, val) in targets.iter().zip(values.iter()) {
                        if let PlaceKind::Ident(name) = &t.kind { self.rebind_alias(&mut local_closures, alias_stack.last_mut().unwrap(), &pre, name, val); }
                    }
                }
                _ => {}
            }
        }
        false
    }

    fn rebind_alias(&self, local_closures: &mut HashMap<String, ClosureDef>, cur: &mut HashSet<String>, pre: &HashSet<String>, name: &str, val: &Expr) {
        match unwrap_parens(val) {
            Expr::FnLit { params, body, .. } => { local_closures.insert(name.to_string(), (params.clone(), body.clone())); }
            Expr::Ident { name: src, .. } => { if let Some(cd) = local_closures.get(src).cloned() { local_closures.insert(name.to_string(), cd); } }
            _ => { local_closures.remove(name); }
        }
        let mut roots = Vec::new();
        self.collect_handle_expr_roots(None, val, &mut roots);
        if roots.iter().any(|r| pre.contains(r)) { cur.insert(name.to_string()); } else { cur.remove(name); }
    }

    fn hint_expr_shape(&self, shapes: &HashMap<String, TypeShape>, e: &Expr) -> Option<TypeShape> {
        match unwrap_parens(e) {
            Expr::Ident { name, .. } => shapes.get(name).cloned(),
            Expr::Try { operand, .. } | Expr::Move { operand, .. } => self.hint_expr_shape(shapes, operand),
            Expr::Field { base, name, .. } => Some(self.field_shape(&self.hint_expr_shape(shapes, base)?, name)).filter(|s| !matches!(s, TypeShape::Unknown)),
            Expr::StructLit { ty: Some(t), .. } => Some(self.shape_of_type(t)),
            Expr::Call { callee, .. } => match unwrap_parens(callee) {
                Expr::Field { base, name, .. } => {
                    if let Expr::Ident { name: pkg, .. } = unwrap_parens(base) {
                        if !shapes.contains_key(pkg) && self.known_package(pkg) { return Some(foreign::package_result(pkg, name)).filter(|s| !matches!(s, TypeShape::Unknown)); }
                    }
                    Some(foreign::method_result(&receiver_type_name(&self.hint_expr_shape(shapes, base)?)?, name)).filter(|s| !matches!(s, TypeShape::Unknown))
                }
                _ => None,
            },
            _ => None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn call_mutates_or_consumes_handle(&self, st_opt: Option<&FnState>, shapes: &HashMap<String, TypeShape>, bound_names: &HashSet<String>, local_closures: &HashMap<String, ClosureDef>, cr: &CallRef, param: &str, depth: usize) -> bool {
        let has_param = |expr: &Expr| { let mut r = Vec::new(); self.collect_handle_expr_roots(None, expr, &mut r); r.iter().any(|x| x == param) };
        let arg_has_param = |a: &hy_syntax::CallArg| has_param(&a.value);
        match unwrap_parens(cr.callee) {
            Expr::Field { base, name: method, .. } => {
                let head = Self::base_named_type_str(method);
                if let Expr::Ident { name: base_id, .. } = unwrap_parens(base) {
                    if !bound_names.contains(base_id) {
                        if self.known_package(base_id) {
                            return !A3_PURE_PKGS.contains(&base_id.as_str()) && matches!(foreign::package_mode(base_id, method), CallMode::Mutate | CallMode::Consume) && cr.args.iter().any(&arg_has_param);
                        }
                        if let Some(&decl) = self.fns.get(format!("{base_id}.{head}").as_str()) { return self.args_mutate_decl(cr, decl, param, depth); }
                    }
                }
                if let Some(bshape) = self.hint_expr_shape(shapes, base) {
                    if let Some(tname) = receiver_type_name(&bshape) {
                        return matches!(foreign::method_mode(&tname, method), CallMode::Mutate | CallMode::Consume) && (has_param(base) || (method == "set" && cr.args.get(1).is_some_and(&arg_has_param)));
                    }
                    if let Some(sb) = struct_base_of(&bshape) {
                        if let Some(&mdecl) = self.method_decls.get(&(sb, head.to_string())) {
                            return (has_param(base) && mdecl.receiver.as_ref().is_some_and(|rp| self.fn_mutates_or_consumes_handle(mdecl, &rp.name, depth + 1))) || self.args_mutate_decl(cr, mdecl, param, depth);
                        }
                    }
                }
                if !self.method_decls.iter().any(|((_, m), _)| m == head) {
                    if matches!(head, "array" | "int" | "string" | "len" | "cap" | "unwrap" | "or" | "is_some" | "is_none" | "insert" | "contains" | "pop") { return false; }
                    if matches!(head, "set" | "push_string" | "write" | "finish") { return has_param(base) || cr.args.iter().any(&arg_has_param); }
                }
                has_param(base) || cr.args.iter().any(&arg_has_param)
            }
            Expr::Ident { name, .. } => {
                let head = Self::base_named_type_str(name);
                if !bound_names.contains(name.as_str()) && !bound_names.contains(head) {
                    if is_pure_builtin_fn(name, head) { return false; }
                    if let Some(&decl) = self.fns.get(name.as_str()).or_else(|| self.fns.get(head)) { return self.args_mutate_decl(cr, decl, param, depth); }
                }
                if let Some((cparams, cbody)) = local_closures.get(name.as_str()).cloned().or_else(|| st_opt.and_then(|st| st.closure_defs.get(name.as_str())).map(|(p, _, b)| (p.clone(), b.clone()))) {
                    return self.closure_call_mutates_param(st_opt, shapes, &cparams, &[], &cbody, cr, param, depth);
                }
                cr.args.iter().any(&arg_has_param)
            }
            Expr::FnLit { params, results, body, .. } => self.closure_call_mutates_param(st_opt, shapes, params, results, body, cr, param, depth),
            _ => has_param(cr.callee) || cr.args.iter().any(&arg_has_param),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn closure_call_mutates_param(&self, st_opt: Option<&FnState>, shapes: &HashMap<String, TypeShape>, params: &[Param], results: &[hy_syntax::FnResult], body: &Block, cr: &CallRef, param: &str, depth: usize) -> bool {
        let (cbound, mut cshapes) = self.closure_env(None, params, results);
        for (k, v) in shapes { cshapes.entry(k.clone()).or_insert_with(|| v.clone()); }
        (!cbound.contains(param) && self.block_mutates_or_consumes_handle(st_opt, &cshapes, &cbound, body, param, depth + 1))
            || cr.args.iter().enumerate().any(|(i, a)| {
                let mut r = Vec::new();
                self.collect_handle_expr_roots(None, &a.value, &mut r);
                r.iter().any(|x| x == param) && params.get(i).map(|cp| self.block_mutates_or_consumes_handle(st_opt, &cshapes, &cbound, body, &cp.name, depth + 1)).unwrap_or(true)
            })
    }

    fn args_mutate_decl(&self, cr: &CallRef, decl: &FnDecl, param: &str, depth: usize) -> bool {
        cr.args.iter().enumerate().any(|(i, a)| {
            let mut r = Vec::new();
            self.collect_handle_expr_roots(None, &a.value, &mut r);
            r.iter().any(|x| x == param) && decl.params.get(i).map(|tp| self.fn_mutates_or_consumes_handle(decl, &tp.name, depth + 1)).unwrap_or(true)
        })
    }
}
