//! Wildcard #1744 Feasibility Spike — Staged Place-Graph Canonicalization (`wrappers -> places -> captures`).
//!
//! Incorporates the staged architecture and 5 pre-registered falsifiers from PR #1934:
//!   - Stage 1 (~400 non-comment LoC in spike; ~280 LoC production delta): Pure AST wrapper &
//!     address-of normalization (`normalize_file`).
//!   - Stage 2 (~960 non-comment LoC in spike; ~1,150 LoC production delta): Canonical `PlaceId` /
//!     `BindingKind` (`UserLocal | ParamOrReceiver | SlotLoan | BorrowView | Temp`) and
//!     `CanonicalPlace` lowering with subexpression read retention (`subexpr_reads`).
//!   - Stage 3 (~220 non-comment LoC in spike; ~750 LoC production delta): Closure `CapturePlace`
//!     summaries (`Read | Mutate | Move`), loan `last_use` extension across closure calls (#1814),
//!     receiver-map invalidation (#1792), and empirical Stage-2 conservatism cost measurement
//!     across the 129-program checker corpus.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

use hy_diag::Span;
use hy_syntax::{
    Block, CallArg, ConstDecl, Decl, Expr, FieldInit, File, FnDecl, ForKind, InterpPart,
    LocalDecl, MatchArm, Place, PlaceKind, SelectArm, SelectArmKind, Stmt, TypeExpr, UnOp,
};

// ===========================================================================
// Stage 1 — Wrapper & Surface Normalization
// ===========================================================================

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Stage1Stats {
    pub parens_erased: usize,
    pub unary_addr_normalized: usize,
}

pub fn normalize_file(file: &File) -> (File, Stage1Stats) {
    let mut stats = Stage1Stats::default();
    let decls = file
        .decls
        .iter()
        .map(|d| normalize_decl(d, &mut stats))
        .collect();
    (
        File {
            path: file.path.clone(),
            decls,
        },
        stats,
    )
}

fn normalize_decl(decl: &Decl, stats: &mut Stage1Stats) -> Decl {
    match decl {
        Decl::Import { path, span } => Decl::Import {
            path: path.clone(),
            span: *span,
        },
        Decl::Const(c) => Decl::Const(ConstDecl {
            name: c.name.clone(),
            ty: c.ty.as_ref().map(|t| normalize_type(t, stats)),
            value: normalize_expr(&c.value, stats),
            span: c.span,
        }),
        Decl::Struct(s) => Decl::Struct(s.clone()),
        Decl::Enum(e) => Decl::Enum(e.clone()),
        Decl::TypeAlias(ta) => Decl::TypeAlias(ta.clone()),
        Decl::Fn(f) => Decl::Fn(FnDecl {
            is_pub: f.is_pub,
            name: f.name.clone(),
            receiver: f.receiver.clone(),
            type_params: f.type_params.clone(),
            params: f.params.clone(),
            results: f.results.clone(),
            body: f.body.as_ref().map(|b| normalize_block(b, stats)),
            span: f.span,
        }),
    }
}

fn normalize_type(ty: &TypeExpr, stats: &mut Stage1Stats) -> TypeExpr {
    match ty {
        TypeExpr::Named { .. } => ty.clone(),
        TypeExpr::Pointer { to, span } => TypeExpr::Pointer {
            to: Box::new(normalize_type(to, stats)),
            span: *span,
        },
        TypeExpr::Slice { elem, span } => TypeExpr::Slice {
            elem: Box::new(normalize_type(elem, stats)),
            span: *span,
        },
        TypeExpr::Array { len, elem, span } => TypeExpr::Array {
            len: len.as_ref().map(|e| Box::new(normalize_expr(e, stats))),
            elem: Box::new(normalize_type(elem, stats)),
            span: *span,
        },
        TypeExpr::Map { key, value, span } => TypeExpr::Map {
            key: Box::new(normalize_type(key, stats)),
            value: Box::new(normalize_type(value, stats)),
            span: *span,
        },
        TypeExpr::Chan { dir, elem, span } => TypeExpr::Chan {
            dir: *dir,
            elem: Box::new(normalize_type(elem, stats)),
            span: *span,
        },
        TypeExpr::Option { elem, span } => TypeExpr::Option {
            elem: Box::new(normalize_type(elem, stats)),
            span: *span,
        },
        TypeExpr::Fn {
            params,
            results,
            span,
        } => TypeExpr::Fn {
            params: params.clone(),
            results: results.iter().map(|r| normalize_type(r, stats)).collect(),
            span: *span,
        },
    }
}

fn normalize_block(block: &Block, stats: &mut Stage1Stats) -> Block {
    Block {
        stmts: block
            .stmts
            .iter()
            .map(|s| normalize_stmt(s, stats))
            .collect(),
        span: block.span,
    }
}

fn normalize_place(place: &Place, stats: &mut Stage1Stats) -> Place {
    let kind = match &place.kind {
        PlaceKind::Ident(n) => PlaceKind::Ident(n.clone()),
        PlaceKind::Index { base, index } => PlaceKind::Index {
            base: Box::new(normalize_place(base, stats)),
            index: normalize_expr(index, stats),
        },
        PlaceKind::Field { base, name } => PlaceKind::Field {
            base: Box::new(normalize_place(base, stats)),
            name: name.clone(),
        },
        PlaceKind::AddrOf { base } => PlaceKind::AddrOf {
            base: Box::new(normalize_place(base, stats)),
        },
    };
    Place {
        kind,
        span: place.span,
    }
}

fn normalize_stmt(stmt: &Stmt, stats: &mut Stage1Stats) -> Stmt {
    match stmt {
        Stmt::Local(l) => Stmt::Local(LocalDecl {
            kind: l.kind,
            names: l.names.clone(),
            ty: l.ty.as_ref().map(|t| normalize_type(t, stats)),
            values: l.values.iter().map(|v| normalize_expr(v, stats)).collect(),
            span: l.span,
        }),
        Stmt::Assign {
            targets,
            op,
            values,
            span,
        } => Stmt::Assign {
            targets: targets.iter().map(|t| normalize_place(t, stats)).collect(),
            op: *op,
            values: values.iter().map(|v| normalize_expr(v, stats)).collect(),
            span: *span,
        },
        Stmt::Expr { expr, span } => Stmt::Expr {
            expr: normalize_expr(expr, stats),
            span: *span,
        },
        Stmt::Send {
            chan,
            value,
            is_move,
            span,
        } => Stmt::Send {
            chan: normalize_expr(chan, stats),
            value: normalize_expr(value, stats),
            is_move: *is_move,
            span: *span,
        },
        Stmt::If {
            init,
            cond,
            then,
            otherwise,
            span,
        } => Stmt::If {
            init: init.as_ref().map(|s| Box::new(normalize_stmt(s, stats))),
            cond: normalize_expr(cond, stats),
            then: normalize_block(then, stats),
            otherwise: otherwise
                .as_ref()
                .map(|s| Box::new(normalize_stmt(s, stats))),
            span: *span,
        },
        Stmt::For { kind, body, span } => {
            let kind = match kind {
                ForKind::Infinite => ForKind::Infinite,
                ForKind::While { cond } => ForKind::While {
                    cond: normalize_expr(cond, stats),
                },
                ForKind::Range {
                    var,
                    lo,
                    hi,
                    inclusive,
                } => ForKind::Range {
                    var: var.clone(),
                    lo: normalize_expr(lo, stats),
                    hi: normalize_expr(hi, stats),
                    inclusive: *inclusive,
                },
                ForKind::Iterate { vars, expr } => ForKind::Iterate {
                    vars: vars.clone(),
                    expr: normalize_expr(expr, stats),
                },
            };
            Stmt::For {
                kind,
                body: normalize_block(body, stats),
                span: *span,
            }
        }
        Stmt::Return { values, span } => Stmt::Return {
            values: values.iter().map(|v| normalize_expr(v, stats)).collect(),
            span: *span,
        },
        Stmt::Task { callee, args, span } => Stmt::Task {
            callee: normalize_expr(callee, stats),
            args: args
                .iter()
                .map(|a| CallArg {
                    name: a.name.clone(),
                    value: normalize_expr(&a.value, stats),
                    spread: a.spread,
                    span: a.span,
                })
                .collect(),
            span: *span,
        },
        Stmt::Managed {
            reason,
            category,
            tickets,
            body,
            span,
        } => Stmt::Managed {
            reason: reason.clone(),
            category: category.clone(),
            tickets: tickets.clone(),
            body: normalize_block(body, stats),
            span: *span,
        },
        Stmt::Match {
            subject,
            arms,
            span,
        } => Stmt::Match {
            subject: normalize_expr(subject, stats),
            arms: arms
                .iter()
                .map(|arm| MatchArm {
                    pattern: arm.pattern.clone(),
                    guard: arm.guard.as_ref().map(|g| normalize_expr(g, stats)),
                    body: normalize_block(&arm.body, stats),
                })
                .collect(),
            span: *span,
        },
        Stmt::Defer { call, span } => Stmt::Defer {
            call: normalize_expr(call, stats),
            span: *span,
        },
        Stmt::Break { .. } | Stmt::Continue { .. } => stmt.clone(),
        Stmt::Block { block, span } => Stmt::Block {
            block: normalize_block(block, stats),
            span: *span,
        },
        Stmt::Select { arms, span } => Stmt::Select {
            arms: arms
                .iter()
                .map(|arm| SelectArm {
                    kind: match &arm.kind {
                        SelectArmKind::Send {
                            chan,
                            value,
                            is_move,
                        } => SelectArmKind::Send {
                            chan: normalize_expr(chan, stats),
                            value: normalize_expr(value, stats),
                            is_move: *is_move,
                        },
                        SelectArmKind::Recv { names, chan } => SelectArmKind::Recv {
                            names: names.clone(),
                            chan: normalize_expr(chan, stats),
                        },
                        SelectArmKind::Default => SelectArmKind::Default,
                    },
                    body: normalize_block(&arm.body, stats),
                })
                .collect(),
            span: *span,
        },
    }
}

fn normalize_expr(expr: &Expr, stats: &mut Stage1Stats) -> Expr {
    match expr {
        Expr::Paren { inner, .. } => {
            stats.parens_erased += 1;
            normalize_expr(inner, stats)
        }
        Expr::Unary {
            op: UnOp::Addr,
            operand,
            span,
        } => {
            stats.unary_addr_normalized += 1;
            Expr::AddrOf {
                operand: Box::new(normalize_expr(operand, stats)),
                span: *span,
            }
        }
        Expr::Unary { op, operand, span } => Expr::Unary {
            op: *op,
            operand: Box::new(normalize_expr(operand, stats)),
            span: *span,
        },
        Expr::AddrOf { operand, span } => Expr::AddrOf {
            operand: Box::new(normalize_expr(operand, stats)),
            span: *span,
        },
        Expr::Move { operand, span } => Expr::Move {
            operand: Box::new(normalize_expr(operand, stats)),
            span: *span,
        },
        Expr::Try { operand, span } => Expr::Try {
            operand: Box::new(normalize_expr(operand, stats)),
            span: *span,
        },
        Expr::Receive { chan, span } => Expr::Receive {
            chan: Box::new(normalize_expr(chan, stats)),
            span: *span,
        },
        Expr::Binary { op, lhs, rhs, span } => Expr::Binary {
            op: *op,
            lhs: Box::new(normalize_expr(lhs, stats)),
            rhs: Box::new(normalize_expr(rhs, stats)),
            span: *span,
        },
        Expr::Index { base, index, span } => Expr::Index {
            base: Box::new(normalize_expr(base, stats)),
            index: Box::new(normalize_expr(index, stats)),
            span: *span,
        },
        Expr::Slice {
            base,
            lo,
            hi,
            span,
        } => Expr::Slice {
            base: Box::new(normalize_expr(base, stats)),
            lo: lo.as_ref().map(|e| Box::new(normalize_expr(e, stats))),
            hi: hi.as_ref().map(|e| Box::new(normalize_expr(e, stats))),
            span: *span,
        },
        Expr::Field { base, name, span } => Expr::Field {
            base: Box::new(normalize_expr(base, stats)),
            name: name.clone(),
            span: *span,
        },
        Expr::Call { callee, args, span } => Expr::Call {
            callee: Box::new(normalize_expr(callee, stats)),
            args: args
                .iter()
                .map(|a| CallArg {
                    name: a.name.clone(),
                    value: normalize_expr(&a.value, stats),
                    spread: a.spread,
                    span: a.span,
                })
                .collect(),
            span: *span,
        },
        Expr::StructLit { ty, fields, span } => Expr::StructLit {
            ty: ty.as_ref().map(|t| normalize_type(t, stats)),
            fields: fields
                .iter()
                .map(|f| FieldInit {
                    name: f.name.clone(),
                    value: normalize_expr(&f.value, stats),
                    span: f.span,
                })
                .collect(),
            span: *span,
        },
        Expr::SliceLit { elem, items, span } => Expr::SliceLit {
            elem: elem.as_ref().map(|t| Box::new(normalize_type(t, stats))),
            items: items.iter().map(|i| normalize_expr(i, stats)).collect(),
            span: *span,
        },
        Expr::MapLit {
            key,
            value,
            entries,
            span,
        } => Expr::MapLit {
            key: Box::new(normalize_type(key, stats)),
            value: Box::new(normalize_type(value, stats)),
            entries: entries
                .iter()
                .map(|(k, v)| (normalize_expr(k, stats), normalize_expr(v, stats)))
                .collect(),
            span: *span,
        },
        Expr::Interp { parts, span } => Expr::Interp {
            parts: parts
                .iter()
                .map(|p| match p {
                    InterpPart::Lit(s) => InterpPart::Lit(s.clone()),
                    InterpPart::Expr(e) => InterpPart::Expr(normalize_expr(e, stats)),
                })
                .collect(),
            span: *span,
        },
        Expr::FnLit {
            params,
            results,
            body,
            span,
        } => Expr::FnLit {
            params: params.clone(),
            results: results.clone(),
            body: normalize_block(body, stats),
            span: *span,
        },
        Expr::Ident { .. } | Expr::Lit { .. } => expr.clone(),
    }
}

// ===========================================================================
// Stage 2 — Canonical PlaceId Graph (`UserLocal | NonOwned | SlotLoan | Temp`)
// & Stage 3 — Closure Capture Places (`Read | Mutate | Move`)
// ===========================================================================

pub type BindingId = usize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TempKind {
    /// Composite literal `T{...}` — addressable in Go (`&T{...}` is legal, `&T{...}.x` is not).
    CompositeLit,
    /// Function/closure call return, map index rvalue, receive, binary expr — NOT addressable in Go.
    RvalueResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingKind {
    UserLocal,
    ParamOrReceiver,
    SlotLoan { map_root: BindingId },
    BorrowView { owner_root: BindingId },
    Temp(TempKind),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Proj {
    Field(String),
    Index,
    Slice,
    Deref,
}

#[derive(Debug, Clone)]
pub struct CanonicalPlace {
    pub root: BindingId,
    pub projections: Vec<Proj>,
    /// Every binding read inside `Index(idx)` / `Slice(lo, hi)` projections along this place.
    pub subexpr_reads: Vec<(BindingId, Span)>,
    pub terminal_is_own_handle: bool,
    pub cur_is_map: bool,
    pub at_map_index: bool,
    pub at_map_value: bool,
    pub has_post_map_value_proj: bool,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureMode {
    Read,
    Mutate,
    Move,
}

#[derive(Debug, Clone)]
pub struct CapturePlace {
    pub outer: BindingId,
    pub mode: CaptureMode,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirDiagnostic {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClosurePolicy {
    /// Stage 2 conservative policy (PR #1934 §3/§4): reject any closure capturing an outer
    /// binding that participates in an active borrow/slot loan in the enclosing function.
    Stage2Conservative,
    /// Stage 3 place-capture policy: distinguish `Read` vs `Mutate` captures, extend outer
    /// loan live windows across closure invocations (#1814), and invalidate live loans when a
    /// closure/helper mutates the loaned container (#1792).
    Stage3CapturePlaces,
}

#[derive(Debug, Clone)]
struct BindingInfo {
    name: String,
    kind: BindingKind,
    is_map: bool,
    is_array_of_maps: bool,
    is_pointer_or_struct: bool,
    is_own_handle: bool,
}

#[derive(Debug, Clone)]
struct ActiveLoan {
    code_on_conflict: &'static str,
    owner_root: BindingId,
    handle_binding: BindingId,
    created_step: usize,
    last_use_step: usize,
}

pub struct PlaceGraphChecker {
    policy: ClosurePolicy,
    struct_fields: HashMap<String, HashMap<String, TypeExpr>>,
    helper_mutates_param0: HashSet<String>,
    bindings: Vec<BindingInfo>,
    scopes: Vec<HashMap<String, BindingId>>,
    may_share: HashMap<BindingId, HashSet<BindingId>>,
    moved: HashMap<BindingId, Span>,
    closures: HashMap<BindingId, Vec<CapturePlace>>,
    loans: Vec<ActiveLoan>,
    binding_read_steps: HashMap<BindingId, Vec<usize>>,
    mutations: Vec<(usize, BindingId, Span)>,
    step: usize,
    pub diagnostics: Vec<MirDiagnostic>,
    pub conservative_closure_rejects: usize,
}

pub fn check_file_place_graph(file: &File, policy: ClosurePolicy) -> PlaceGraphChecker {
    let (norm, _) = normalize_file(file);
    let mut struct_fields: HashMap<String, HashMap<String, TypeExpr>> = HashMap::new();
    for d in &norm.decls {
        if let Decl::Struct(s) = d {
            let mut fs = HashMap::new();
            for f in &s.fields {
                fs.insert(f.name.clone(), f.ty.clone());
            }
            struct_fields.insert(s.name.clone(), fs);
        }
    }

    // Lightweight summary for helper functions/methods that mutate receiver or param 0 (#1792).
    let mut helper_mutates_param0 = HashSet::new();
    for f in norm.fns() {
        if let Some(body) = &f.body {
            let target_param = f
                .receiver
                .as_ref()
                .map(|r| r.name.as_str())
                .or_else(|| f.params.first().map(|p| p.name.as_str()));
            if let Some(pname) = target_param {
                if block_mutates_root_name(body, pname) {
                    helper_mutates_param0.insert(f.name.clone());
                }
            }
        }
    }

    let mut ck = PlaceGraphChecker {
        policy,
        struct_fields,
        helper_mutates_param0,
        bindings: Vec::new(),
        scopes: vec![HashMap::new()],
        may_share: HashMap::new(),
        moved: HashMap::new(),
        closures: HashMap::new(),
        loans: Vec::new(),
        binding_read_steps: HashMap::new(),
        mutations: Vec::new(),
        step: 0,
        diagnostics: Vec::new(),
        conservative_closure_rejects: 0,
    };

    for f in norm.fns() {
        ck.check_fn(f);
    }
    ck
}

fn block_mutates_root_name(block: &Block, root: &str) -> bool {
    block.stmts.iter().any(|s| stmt_mutates_root_name(s, root))
}

fn stmt_mutates_root_name(stmt: &Stmt, root: &str) -> bool {
    match stmt {
        Stmt::Assign { targets, .. } => targets.iter().any(|t| {
            t.root() == root && matches!(t.kind, PlaceKind::Index { .. } | PlaceKind::Field { .. })
        }),
        Stmt::Expr {
            expr: Expr::Call { callee, args, .. },
            ..
        } => {
            if let Expr::Ident { name, .. } = callee.as_ref() {
                if name == "delete"
                    && args
                        .first()
                        .is_some_and(|a| a.value.root_ident() == Some(root))
                {
                    return true;
                }
            }
            if let Expr::Field { base, name, .. } = callee.as_ref() {
                if base.root_ident() == Some(root)
                    && matches!(name.as_str(), "set" | "push_string" | "write" | "put" | "bump")
                {
                    return true;
                }
            }
            false
        }
        Stmt::If {
            then, otherwise, ..
        } => {
            block_mutates_root_name(then, root)
                || otherwise
                    .as_deref()
                    .is_some_and(|o| stmt_mutates_root_name(o, root))
        }
        Stmt::For { body, .. } | Stmt::Managed { body, .. } | Stmt::Block { block: body, .. } => {
            block_mutates_root_name(body, root)
        }
        _ => false,
    }
}

fn is_own_handle_type(ty: &TypeExpr) -> bool {
    matches!(
        ty,
        TypeExpr::Named { name, .. }
            if matches!(name.as_str(), "serde_json.value" | "flate2.writer")
    )
}

fn is_map_or_ptr_to_map(ty: &TypeExpr) -> bool {
    match ty {
        TypeExpr::Map { .. } => true,
        TypeExpr::Pointer { to, .. } => matches!(to.as_ref(), TypeExpr::Map { .. }),
        _ => false,
    }
}

fn is_array_of_maps_type(ty: &TypeExpr) -> bool {
    match ty {
        TypeExpr::Array { elem, .. } => is_map_or_ptr_to_map(elem),
        TypeExpr::Pointer { to, .. } => matches!(
            to.as_ref(),
            TypeExpr::Array { elem, .. } if is_map_or_ptr_to_map(elem)
        ),
        _ => false,
    }
}

impl PlaceGraphChecker {
    fn alloc_binding(
        &mut self,
        name: impl Into<String>,
        kind: BindingKind,
        is_map: bool,
        is_pointer_or_struct: bool,
        is_own_handle: bool,
    ) -> BindingId {
        self.alloc_binding_full(name, kind, is_map, false, is_pointer_or_struct, is_own_handle)
    }

    fn alloc_binding_full(
        &mut self,
        name: impl Into<String>,
        kind: BindingKind,
        is_map: bool,
        is_array_of_maps: bool,
        is_pointer_or_struct: bool,
        is_own_handle: bool,
    ) -> BindingId {
        let id = self.bindings.len();
        let name_s = name.into();
        self.bindings.push(BindingInfo {
            name: name_s.clone(),
            kind,
            is_map,
            is_array_of_maps,
            is_pointer_or_struct,
            is_own_handle,
        });
        if !name_s.starts_with("_tmp$") && name_s != "_" {
            if let Some(scope) = self.scopes.last_mut() {
                scope.insert(name_s, id);
            }
        }
        id
    }

    fn lookup(&self, name: &str) -> Option<BindingId> {
        for s in self.scopes.iter().rev() {
            if let Some(&id) = s.get(name) {
                return Some(id);
            }
        }
        None
    }

    fn add_share_edge(&mut self, a: BindingId, b: BindingId) {
        self.may_share.entry(a).or_default().insert(b);
        self.may_share.entry(b).or_default().insert(a);
    }

    fn share_component(&self, start: BindingId) -> BTreeSet<BindingId> {
        let mut seen = BTreeSet::new();
        let mut stack = vec![start];
        while let Some(cur) = stack.pop() {
            if seen.insert(cur) {
                if let Some(peers) = self.may_share.get(&cur) {
                    for &p in peers {
                        if !seen.contains(&p) {
                            stack.push(p);
                        }
                    }
                }
            }
        }
        seen
    }

    fn record_read(&mut self, id: BindingId, span: Span) {
        self.binding_read_steps
            .entry(id)
            .or_default()
            .push(self.step);
        if let Some(&moved_at) = self.moved.get(&id) {
            let _ = moved_at;
            self.diagnostics.push(MirDiagnostic {
                code: "HYB-E3003",
                message: format!(
                    "use of `{}` after its storage was moved",
                    self.bindings[id].name
                ),
                span,
            });
        }
    }

    fn freeze_component(&mut self, root: BindingId, span: Span) {
        for peer in self.share_component(root) {
            self.moved.entry(peer).or_insert(span);
        }
    }

    fn check_fn(&mut self, f: &FnDecl) {
        let Some(body) = &f.body else { return };
        self.scopes = vec![HashMap::new()];
        self.moved.clear();
        self.closures.clear();
        self.loans.clear();
        self.binding_read_steps.clear();
        self.mutations.clear();
        self.step = 0;

        if let Some(r) = &f.receiver {
            self.alloc_binding_full(
                &r.name,
                BindingKind::ParamOrReceiver,
                is_map_or_ptr_to_map(&r.ty),
                is_array_of_maps_type(&r.ty),
                true,
                is_own_handle_type(&r.ty),
            );
        }
        for p in &f.params {
            self.alloc_binding_full(
                &p.name,
                BindingKind::ParamOrReceiver,
                is_map_or_ptr_to_map(&p.ty),
                is_array_of_maps_type(&p.ty),
                matches!(p.ty, TypeExpr::Pointer { .. } | TypeExpr::Named { .. }),
                is_own_handle_type(&p.ty),
            );
        }

        self.walk_block(body, false);
        self.verify_loan_windows();
    }

    fn verify_loan_windows(&mut self) {
        for loan in &self.loans {
            let last_use = self
                .binding_read_steps
                .get(&loan.handle_binding)
                .and_then(|steps| steps.iter().copied().max())
                .unwrap_or(loan.created_step)
                .max(loan.last_use_step);
            let owner_peers = self.share_component(loan.owner_root);
            for &(m_step, m_root, m_span) in &self.mutations {
                if m_step > loan.created_step && m_step < last_use && owner_peers.contains(&m_root) {
                    self.diagnostics.push(MirDiagnostic {
                        code: loan.code_on_conflict,
                        message: format!(
                            "mutation of `{}` while loan `{}` is still live",
                            self.bindings[m_root].name, self.bindings[loan.handle_binding].name
                        ),
                        span: m_span,
                    });
                }
            }
        }
    }

    fn walk_block(&mut self, block: &Block, in_managed: bool) {
        self.scopes.push(HashMap::new());
        for s in &block.stmts {
            self.step += 1;
            self.walk_stmt(s, in_managed);
        }
        self.scopes.pop();
    }

    fn walk_stmt(&mut self, stmt: &Stmt, in_managed: bool) {
        match stmt {
            Stmt::Local(l) => self.walk_local(l),
            Stmt::Assign {
                targets, values, ..
            } => {
                for v in values {
                    self.eval_expr(v);
                }
                for t in targets {
                    self.walk_assign_place(t);
                }
            }
            Stmt::Expr { expr, .. } => {
                self.eval_expr(expr);
            }
            Stmt::Send {
                chan,
                value,
                is_move,
                span,
            } => {
                self.eval_expr(chan);
                if *is_move {
                    self.check_move_transfer(value, *span, in_managed);
                } else {
                    self.eval_expr(value);
                }
            }
            Stmt::Task { callee, args, .. } => {
                self.eval_expr(callee);
                for a in args {
                    if let Expr::Move { operand, span } = &a.value {
                        self.check_move_transfer(operand, *span, in_managed);
                    } else {
                        self.eval_expr(&a.value);
                    }
                }
            }
            Stmt::Managed { body, .. } => self.walk_block(body, true),
            Stmt::If {
                init,
                cond,
                then,
                otherwise,
                ..
            } => {
                self.scopes.push(HashMap::new());
                if let Some(i) = init {
                    self.walk_stmt(i, in_managed);
                }
                self.eval_expr(cond);
                self.walk_block(then, in_managed);
                if let Some(o) = otherwise {
                    self.walk_stmt(o, in_managed);
                }
                self.scopes.pop();
            }
            Stmt::For { kind, body, .. } => {
                self.scopes.push(HashMap::new());
                match kind {
                    ForKind::Infinite => {}
                    ForKind::While { cond } => {
                        self.eval_expr(cond);
                    }
                    ForKind::Range { var, lo, hi, .. } => {
                        self.eval_expr(lo);
                        self.eval_expr(hi);
                        self.alloc_binding(var, BindingKind::UserLocal, false, false, false);
                    }
                    ForKind::Iterate { vars, expr } => {
                        self.eval_expr(expr);
                        for v in vars {
                            self.alloc_binding(v, BindingKind::ParamOrReceiver, false, false, false);
                        }
                    }
                }
                self.walk_block(body, in_managed);
                self.scopes.pop();
            }
            Stmt::Match { subject, arms, .. } => {
                self.eval_expr(subject);
                for arm in arms {
                    self.scopes.push(HashMap::new());
                    if let hy_syntax::MatchPattern::Path {
                        binding: Some(b), ..
                    } = &arm.pattern
                    {
                        self.alloc_binding(b, BindingKind::UserLocal, false, false, false);
                    }
                    if let Some(g) = &arm.guard {
                        self.eval_expr(g);
                    }
                    self.walk_block(&arm.body, in_managed);
                    self.scopes.pop();
                }
            }
            Stmt::Select { arms, .. } => {
                // All select arm channel & send operands evaluate before arm selection (#1778).
                for arm in arms {
                    match &arm.kind {
                        SelectArmKind::Send {
                            chan,
                            value,
                            is_move,
                        } => {
                            self.eval_expr(chan);
                            if *is_move {
                                self.check_move_transfer(value, value.span(), in_managed);
                            } else {
                                self.eval_expr(value);
                            }
                        }
                        SelectArmKind::Recv { chan, .. } => {
                            self.eval_expr(chan);
                        }
                        SelectArmKind::Default => {}
                    }
                }
                for arm in arms {
                    self.scopes.push(HashMap::new());
                    if let SelectArmKind::Recv { names, .. } = &arm.kind {
                        for n in names {
                            self.alloc_binding(n, BindingKind::UserLocal, false, false, false);
                        }
                    }
                    self.walk_block(&arm.body, in_managed);
                    self.scopes.pop();
                }
            }
            Stmt::Return { values, .. } => {
                for v in values {
                    self.eval_expr(v);
                }
            }
            Stmt::Defer { call, .. } => {
                self.eval_expr(call);
            }
            Stmt::Block { block, .. } => self.walk_block(block, in_managed),
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
        }
    }

    fn walk_local(&mut self, l: &LocalDecl) {
        if l.names.len() == 1 && l.values.len() == 1 {
            let name = &l.names[0];
            let val = &l.values[0];

            // O8 Map-slot loan binding: `n := &m[k]` or `entry := &s.m[k]`
            if let Expr::AddrOf { operand, .. } = val {
                let place = self.lower_place(operand);
                for (rid, rspan) in &place.subexpr_reads {
                    self.record_read(*rid, *rspan);
                }
                if place.at_map_index {
                    self.record_read(place.root, val.span());
                    let id = self.alloc_binding(
                        name,
                        BindingKind::SlotLoan {
                            map_root: place.root,
                        },
                        false,
                        true,
                        false,
                    );
                    self.loans.push(ActiveLoan {
                        code_on_conflict: "HYB-E3102",
                        owner_root: place.root,
                        handle_binding: id,
                        created_step: self.step,
                        last_use_step: self.step,
                    });
                    return;
                }
            }

            // O7 Borrow-view binding: `view := doc.array("k")?`
            if let Some(owner_id) = self.as_borrow_view_call(val) {
                self.record_read(owner_id, val.span());
                let id = self.alloc_binding(
                    name,
                    BindingKind::BorrowView {
                        owner_root: owner_id,
                    },
                    false,
                    false,
                    false,
                );
                self.loans.push(ActiveLoan {
                    code_on_conflict: "HYB-E3101",
                    owner_root: owner_id,
                    handle_binding: id,
                    created_step: self.step,
                    last_use_step: self.step,
                });
                return;
            }

            // Closure literal binding: `f := fn(...) { ... }`
            if let Expr::FnLit { params, body, span, .. } = val {
                let captures = self.summarize_closure(params, body, *span);
                let id = self.alloc_binding(name, BindingKind::UserLocal, false, false, false);
                self.closures.insert(id, captures);
                return;
            }

            // Local move chain (`b := move a`, #1814): freeze `a` AND link `b` to `a`'s MayShare peers.
            if let Expr::Move { operand, span } = val {
                let place = self.lower_place(operand);
                for (rid, rspan) in &place.subexpr_reads {
                    self.record_read(*rid, *rspan);
                }
                let src_peers = self.share_component(place.root);
                self.freeze_component(place.root, *span);
                let id = self.alloc_binding(
                    name,
                    BindingKind::UserLocal,
                    self.bindings[place.root].is_map,
                    self.bindings[place.root].is_pointer_or_struct,
                    self.bindings[place.root].is_own_handle,
                );
                for peer in src_peers {
                    if peer != place.root {
                        self.add_share_edge(id, peer);
                    }
                }
                return;
            }

            let src_id = self.eval_expr(val);
            let is_map = matches!(val, Expr::MapLit { .. })
                || matches!(&l.ty, Some(TypeExpr::Map { .. }))
                || src_id.is_some_and(|s| self.bindings[s].is_map);
            let is_own = l.ty.as_ref().is_some_and(is_own_handle_type)
                || self.is_own_handle_producer(val)
                || src_id.is_some_and(|s| self.bindings[s].is_own_handle);
            let inherited_kind = match src_id.map(|s| &self.bindings[s].kind) {
                Some(BindingKind::SlotLoan { map_root }) => BindingKind::SlotLoan {
                    map_root: *map_root,
                },
                _ => BindingKind::UserLocal,
            };
            let id = self.alloc_binding(name, inherited_kind, is_map, true, is_own);
            if let Some(s) = src_id {
                self.add_share_edge(id, s);
            }
            return;
        }

        for v in &l.values {
            self.eval_expr(v);
        }
        let is_map = l
            .ty
            .as_ref()
            .is_some_and(|t| matches!(t, TypeExpr::Map { .. }));
        let is_own = l.ty.as_ref().is_some_and(is_own_handle_type);
        for name in &l.names {
            self.alloc_binding(name, BindingKind::UserLocal, is_map, true, is_own);
        }
    }

    fn walk_assign_place(&mut self, place: &Place) {
        match &place.kind {
            PlaceKind::Ident(name) => {
                if let Some(id) = self.lookup(name) {
                    self.record_read(id, place.span);
                }
            }
            PlaceKind::Index { base, index } => {
                self.eval_expr(index);
                if let Some(root_id) = self.lookup(base.root()) {
                    self.record_read(root_id, place.span);
                    self.mutations.push((self.step, root_id, place.span));
                }
            }
            PlaceKind::Field { base, .. } | PlaceKind::AddrOf { base } => {
                if let Some(root_id) = self.lookup(base.root()) {
                    self.record_read(root_id, place.span);
                    self.mutations.push((self.step, root_id, place.span));
                }
            }
        }
    }

    /// Uniform Stage-2 `move` transfer rule:
    /// 1. Evaluates all `subexpr_reads` along the place chain (`#1778`).
    /// 2. Checks O5.4 place legality (`UserLocal` whole binding or own-handle field chain)
    ///    uniformly inside AND outside `managed` (`#1742`).
    /// 3. Freezes the entire `MayShare` storage component (`#1814`).
    fn check_move_transfer(&mut self, operand: &Expr, span: Span, _in_managed: bool) {
        let place = self.lower_place(operand);
        for (rid, rspan) in &place.subexpr_reads {
            self.record_read(*rid, *rspan);
        }
        if self.moved.contains_key(&place.root) {
            self.diagnostics.push(MirDiagnostic {
                code: "HYB-E3003",
                message: format!(
                    "use of `{}` after ownership was already moved",
                    self.bindings[place.root].name
                ),
                span,
            });
            return;
        }

        let root_kind = &self.bindings[place.root].kind;
        let is_whole_user_local =
            matches!(root_kind, BindingKind::UserLocal) && place.projections.is_empty();
        let is_own_handle_field = matches!(root_kind, BindingKind::UserLocal)
            && !place.projections.is_empty()
            && place.terminal_is_own_handle;

        if !is_whole_user_local && !is_own_handle_field {
            self.diagnostics.push(MirDiagnostic {
                code: "HYB-E3004",
                message: format!(
                    "cannot `move` non-owned or projected GC place rooted at `{}` (O5.4)",
                    self.bindings[place.root].name
                ),
                span,
            });
        }
        self.freeze_component(place.root, span);
    }

    /// Lower any lvalue/rvalue expression to a `CanonicalPlace`, materializing
    /// `BindingKind::Temp` for composite literals and call results while preserving
    /// every `Index` / `Slice` subexpression read along the projection chain.
    pub fn lower_place(&mut self, expr: &Expr) -> CanonicalPlace {
        match expr {
            Expr::Ident { name, span } => {
                let root = self.lookup(name).unwrap_or_else(|| {
                    self.alloc_binding(
                        format!("_tmp$unresolved_{name}"),
                        BindingKind::Temp(TempKind::RvalueResult),
                        false,
                        false,
                        false,
                    )
                });
                let is_own = self.bindings[root].is_own_handle;
                let cur_is_map = self.bindings[root].is_map;
                CanonicalPlace {
                    root,
                    projections: Vec::new(),
                    subexpr_reads: Vec::new(),
                    terminal_is_own_handle: is_own,
                    cur_is_map,
                    at_map_index: false,
                    at_map_value: false,
                    has_post_map_value_proj: false,
                    span: *span,
                }
            }
            Expr::Field { base, name, span } => {
                let mut p = self.lower_place(base);
                let field_is_own = self.field_is_own_handle(name);
                if p.at_map_value {
                    p.has_post_map_value_proj = true;
                }
                p.at_map_index = false;
                if self.field_breaks_value_containment(name) {
                    p.at_map_value = false;
                }
                p.cur_is_map = self.field_is_map(name);
                p.projections.push(Proj::Field(name.clone()));
                p.terminal_is_own_handle = field_is_own;
                p.span = *span;
                p
            }
            Expr::Index { base, index, span } => {
                let mut p = self.lower_place(base);
                self.collect_expr_reads(index, &mut p.subexpr_reads);
                if p.cur_is_map {
                    p.at_map_index = true;
                    p.at_map_value = true;
                    p.cur_is_map = false;
                } else {
                    if p.at_map_value {
                        p.has_post_map_value_proj = true;
                    }
                    p.at_map_index = false;
                    if p.projections.is_empty() && self.bindings[p.root].is_array_of_maps {
                        p.cur_is_map = true;
                    }
                }
                p.projections.push(Proj::Index);
                p.terminal_is_own_handle = false;
                p.span = *span;
                p
            }
            Expr::Slice {
                base, lo, hi, span, ..
            } => {
                let mut p = self.lower_place(base);
                if let Some(l) = lo {
                    self.collect_expr_reads(l, &mut p.subexpr_reads);
                }
                if let Some(h) = hi {
                    self.collect_expr_reads(h, &mut p.subexpr_reads);
                }
                if p.at_map_value {
                    p.has_post_map_value_proj = true;
                }
                p.at_map_index = false;
                p.cur_is_map = false;
                p.projections.push(Proj::Slice);
                p.terminal_is_own_handle = false;
                p.span = *span;
                p
            }
            Expr::Try { operand, .. } => self.lower_place(operand),
            Expr::StructLit { fields, span, .. } => {
                let mut reads = Vec::new();
                let mut shared_roots = Vec::new();
                for f in fields {
                    if let Some(fid) = self.eval_expr(&f.value) {
                        shared_roots.push(fid);
                    }
                    self.collect_expr_reads(&f.value, &mut reads);
                }
                let tmp = self.alloc_binding(
                    "_tmp$struct_lit",
                    BindingKind::Temp(TempKind::CompositeLit),
                    false,
                    true,
                    false,
                );
                for sr in shared_roots {
                    self.add_share_edge(tmp, sr);
                }
                CanonicalPlace {
                    root: tmp,
                    projections: Vec::new(),
                    subexpr_reads: reads,
                    terminal_is_own_handle: false,
                    cur_is_map: false,
                    at_map_index: false,
                    at_map_value: false,
                    has_post_map_value_proj: false,
                    span: *span,
                }
            }
            other => {
                let _ = self.eval_expr(other);
                let tmp = self.alloc_binding(
                    "_tmp$rvalue",
                    BindingKind::Temp(TempKind::RvalueResult),
                    false,
                    false,
                    self.is_own_handle_producer(other),
                );
                CanonicalPlace {
                    root: tmp,
                    projections: Vec::new(),
                    subexpr_reads: Vec::new(),
                    terminal_is_own_handle: self.bindings[tmp].is_own_handle,
                    cur_is_map: false,
                    at_map_index: false,
                    at_map_value: false,
                    has_post_map_value_proj: false,
                    span: other.span(),
                }
            }
        }
    }

    fn field_is_own_handle(&self, field_name: &str) -> bool {
        self.struct_fields
            .values()
            .any(|fs| fs.get(field_name).is_some_and(is_own_handle_type))
    }

    fn field_is_map(&self, field_name: &str) -> bool {
        self.struct_fields
            .values()
            .any(|fs| fs.get(field_name).is_some_and(is_map_or_ptr_to_map))
    }

    fn field_breaks_value_containment(&self, field_name: &str) -> bool {
        self.struct_fields.values().any(|fs| {
            fs.get(field_name)
                .is_some_and(|t| matches!(t, TypeExpr::Pointer { .. } | TypeExpr::Slice { .. }))
        })
    }

    fn collect_expr_reads(&self, expr: &Expr, out: &mut Vec<(BindingId, Span)>) {
        match expr {
            Expr::Ident { name, span } => {
                if let Some(id) = self.lookup(name) {
                    out.push((id, *span));
                }
            }
            Expr::Index { base, index, .. } => {
                self.collect_expr_reads(base, out);
                self.collect_expr_reads(index, out);
            }
            Expr::Field { base, .. }
            | Expr::Unary { operand: base, .. }
            | Expr::AddrOf { operand: base, .. }
            | Expr::Move { operand: base, .. }
            | Expr::Try { operand: base, .. }
            | Expr::Receive { chan: base, .. }
            | Expr::Paren { inner: base, .. } => self.collect_expr_reads(base, out),
            Expr::Binary { lhs, rhs, .. } => {
                self.collect_expr_reads(lhs, out);
                self.collect_expr_reads(rhs, out);
            }
            Expr::Call { callee, args, .. } => {
                self.collect_expr_reads(callee, out);
                for a in args {
                    self.collect_expr_reads(&a.value, out);
                }
            }
            _ => {}
        }
    }

    fn eval_expr(&mut self, expr: &Expr) -> Option<BindingId> {
        match expr {
            Expr::Ident { name, span } => {
                let id = self.lookup(name)?;
                self.record_read(id, *span);
                Some(id)
            }
            Expr::AddrOf { operand, span } => {
                let place = self.lower_place(operand);
                self.record_read(place.root, *span);
                for (rid, rspan) in &place.subexpr_reads {
                    self.record_read(*rid, *rspan);
                }
                // Uniform map-element sub-part addressability rule (#1820):
                // `&m[k].field`, `(&m[k].arr[0])`, and `&arr[0][k].field` are HYB-E2010
                // anywhere in any expression tree!
                if place.has_post_map_value_proj {
                    self.diagnostics.push(MirDiagnostic {
                        code: "HYB-E2010",
                        message: "cannot take address of field/element inside unaddressable map value".to_string(),
                        span: *span,
                    });
                    return None;
                }
                match &self.bindings[place.root].kind {
                    // Uniform T24 rule (#1775): `&loan` or `&loan.<any projection>` is illegal!
                    BindingKind::SlotLoan { .. } => {
                        self.diagnostics.push(MirDiagnostic {
                            code: "HYB-E3104",
                            message: format!(
                                "cannot take address of map-slot loan `{}` or any projection of it",
                                self.bindings[place.root].name
                            ),
                            span: *span,
                        });
                    }
                    // Uniform Go addressability rule (#1820): `&T{}` (0 projections on CompositeLit)
                    // is addressable in Go, whereas `&T{}.x` or `&(call().x)` is NOT addressable!
                    BindingKind::Temp(TempKind::RvalueResult) => {
                        self.diagnostics.push(MirDiagnostic {
                            code: "HYB-E2012",
                            message: "cannot take address of non-addressable temporary / call return projection".to_string(),
                            span: *span,
                        });
                    }
                    BindingKind::Temp(TempKind::CompositeLit) if !place.projections.is_empty() => {
                        self.diagnostics.push(MirDiagnostic {
                            code: "HYB-E2012",
                            message: "cannot take address of field/index projection on a composite literal temporary".to_string(),
                            span: *span,
                        });
                    }
                    _ => {}
                }
                None
            }
            Expr::Move { operand, span } => {
                self.check_move_transfer(operand, *span, false);
                None
            }
            Expr::Field { base, .. } => self.eval_expr(base),
            Expr::Index { base, index, .. } => {
                let b = self.eval_expr(base);
                self.eval_expr(index);
                b
            }
            Expr::Slice { base, lo, hi, .. } => {
                let b = self.eval_expr(base);
                if let Some(l) = lo {
                    self.eval_expr(l);
                }
                if let Some(h) = hi {
                    self.eval_expr(h);
                }
                b
            }
            Expr::Call { callee, args, span } => {
                self.eval_call(callee, args, *span);
                None
            }
            Expr::FnLit {
                params, body, span, ..
            } => {
                let _ = self.summarize_closure(params, body, *span);
                None
            }
            Expr::StructLit { fields, .. } => {
                let tmp = self.alloc_binding(
                    "_tmp$struct",
                    BindingKind::Temp(TempKind::CompositeLit),
                    false,
                    true,
                    false,
                );
                for f in fields {
                    if let Some(fid) = self.eval_expr(&f.value) {
                        self.add_share_edge(tmp, fid);
                    }
                }
                Some(tmp)
            }
            Expr::SliceLit { items, .. } => {
                let tmp = self.alloc_binding(
                    "_tmp$slice",
                    BindingKind::Temp(TempKind::CompositeLit),
                    false,
                    true,
                    false,
                );
                for it in items {
                    if let Some(iid) = self.eval_expr(it) {
                        self.add_share_edge(tmp, iid);
                    }
                }
                Some(tmp)
            }
            Expr::MapLit { entries, .. } => {
                let tmp = self.alloc_binding(
                    "_tmp$map",
                    BindingKind::Temp(TempKind::CompositeLit),
                    true,
                    true,
                    false,
                );
                for (k, v) in entries {
                    self.eval_expr(k);
                    if let Some(vid) = self.eval_expr(v) {
                        self.add_share_edge(tmp, vid);
                    }
                }
                Some(tmp)
            }
            Expr::Binary { lhs, rhs, .. } => {
                self.eval_expr(lhs);
                self.eval_expr(rhs);
                None
            }
            Expr::Unary { operand, .. }
            | Expr::Try { operand, .. }
            | Expr::Receive { chan: operand, .. }
            | Expr::Paren { inner: operand, .. } => self.eval_expr(operand),
            Expr::Interp { parts, .. } => {
                for p in parts {
                    if let InterpPart::Expr(e) = p {
                        self.eval_expr(e);
                    }
                }
                None
            }
            Expr::Lit { .. } => None,
        }
    }

    fn eval_call(&mut self, callee: &Expr, args: &[CallArg], span: Span) {
        // Closure variable invocation (`f()`): apply Stage 3 capture effects at the call step!
        if let Expr::Ident { name, .. } = callee {
            if let Some(cid) = self.lookup(name) {
                self.record_read(cid, span);
                if let Some(caps) = self.closures.get(&cid).cloned() {
                    for cap in caps {
                        self.record_read(cap.outer, span);
                        if cap.mode == CaptureMode::Mutate {
                            self.mutations.push((self.step, cap.outer, span));
                        } else if cap.mode == CaptureMode::Move {
                            self.freeze_component(cap.outer, span);
                        }
                    }
                }
            } else if self.helper_mutates_param0.contains(name) {
                if let Some(first) = args.first() {
                    if let Some(root_name) = first.value.root_ident() {
                        if let Some(rid) = self.lookup(root_name) {
                            self.mutations.push((self.step, rid, span));
                        }
                    }
                }
            }
        } else if let Expr::Field { base, name, .. } = callee {
            let recv_id = self.eval_expr(base);
            if let Some(rid) = recv_id {
                if matches!(
                    name.as_str(),
                    "set" | "push_string" | "write" | "finish" | "put" | "bump"
                ) || self.helper_mutates_param0.contains(name)
                {
                    self.mutations.push((self.step, rid, span));
                }
            }
        } else {
            self.eval_expr(callee);
        }

        for a in args {
            if let Expr::Move { operand, span: mspan } = &a.value {
                self.check_move_transfer(operand, *mspan, false);
            } else {
                self.eval_expr(&a.value);
            }
        }
    }

    fn summarize_closure(
        &mut self,
        params: &[hy_syntax::Param],
        body: &Block,
        span: Span,
    ) -> Vec<CapturePlace> {
        let param_names: HashSet<&str> = params.iter().map(|p| p.name.as_str()).collect();
        let mut modes: BTreeMap<BindingId, CaptureMode> = BTreeMap::new();
        self.scan_closure_block(body, &param_names, &mut HashSet::new(), &mut modes);

        let mut captures = Vec::new();
        for (outer, mode) in modes {
            if self.policy == ClosurePolicy::Stage2Conservative {
                let touches_loan = self.loans.iter().any(|l| {
                    l.handle_binding == outer
                        || l.owner_root == outer
                        || self.share_component(l.owner_root).contains(&outer)
                });
                if touches_loan {
                    self.conservative_closure_rejects += 1;
                    self.diagnostics.push(MirDiagnostic {
                        code: "HYB-STAGE2-CONSERVATIVE-CAPTURE",
                        message: format!(
                            "Stage-2 conservative closure capture of loan-participating binding `{}`",
                            self.bindings[outer].name
                        ),
                        span,
                    });
                }
            }
            captures.push(CapturePlace { outer, mode, span });
        }
        captures
    }

    fn scan_closure_block(
        &self,
        block: &Block,
        params: &HashSet<&str>,
        locals: &mut HashSet<String>,
        modes: &mut BTreeMap<BindingId, CaptureMode>,
    ) {
        for s in &block.stmts {
            match s {
                Stmt::Local(l) => {
                    for v in &l.values {
                        self.scan_closure_expr(v, params, locals, modes);
                    }
                    for n in &l.names {
                        locals.insert(n.clone());
                    }
                }
                Stmt::Assign {
                    targets, values, ..
                } => {
                    for v in values {
                        self.scan_closure_expr(v, params, locals, modes);
                    }
                    for t in targets {
                        let root = t.root();
                        if !params.contains(root) && !locals.contains(root) {
                            if let Some(id) = self.lookup(root) {
                                modes.insert(id, CaptureMode::Mutate);
                            }
                        }
                    }
                }
                Stmt::Expr { expr, .. } => self.scan_closure_expr(expr, params, locals, modes),
                Stmt::Return { values, .. } => {
                    for v in values {
                        self.scan_closure_expr(v, params, locals, modes);
                    }
                }
                Stmt::If {
                    init,
                    cond,
                    then,
                    otherwise,
                    ..
                } => {
                    if let Some(i) = init {
                        let b = Block {
                            stmts: vec![(**i).clone()],
                            span: then.span,
                        };
                        self.scan_closure_block(&b, params, locals, modes);
                    }
                    self.scan_closure_expr(cond, params, locals, modes);
                    self.scan_closure_block(then, params, locals, modes);
                    if let Some(o) = otherwise {
                        let b = Block {
                            stmts: vec![(**o).clone()],
                            span: then.span,
                        };
                        self.scan_closure_block(&b, params, locals, modes);
                    }
                }
                Stmt::For { body, .. }
                | Stmt::Managed { body, .. }
                | Stmt::Block { block: body, .. } => {
                    self.scan_closure_block(body, params, locals, modes);
                }
                Stmt::Task { callee, args, .. } => {
                    self.scan_closure_expr(callee, params, locals, modes);
                    for a in args {
                        self.scan_closure_expr(&a.value, params, locals, modes);
                    }
                }
                Stmt::Send {
                    chan,
                    value,
                    is_move,
                    ..
                } => {
                    self.scan_closure_expr(chan, params, locals, modes);
                    if *is_move {
                        if let Some(root) = value.root_ident() {
                            if !params.contains(root) && !locals.contains(root) {
                                if let Some(id) = self.lookup(root) {
                                    modes.insert(id, CaptureMode::Move);
                                }
                            }
                        }
                    }
                    self.scan_closure_expr(value, params, locals, modes);
                }
                _ => {}
            }
        }
    }

    fn scan_closure_expr(
        &self,
        expr: &Expr,
        params: &HashSet<&str>,
        locals: &HashSet<String>,
        modes: &mut BTreeMap<BindingId, CaptureMode>,
    ) {
        match expr {
            Expr::Ident { name, .. } => {
                if !params.contains(name.as_str()) && !locals.contains(name) {
                    if let Some(id) = self.lookup(name) {
                        modes.entry(id).or_insert(CaptureMode::Read);
                    }
                }
            }
            Expr::Move { operand, .. } => {
                if let Some(root) = operand.root_ident() {
                    if !params.contains(root) && !locals.contains(root) {
                        if let Some(id) = self.lookup(root) {
                            modes.insert(id, CaptureMode::Move);
                        }
                    }
                }
                self.scan_closure_expr(operand, params, locals, modes);
            }
            Expr::Call { callee, args, .. } => {
                if let Expr::Field { base, name, .. } = callee.as_ref() {
                    if let Some(root) = base.root_ident() {
                        if !params.contains(root) && !locals.contains(root) {
                            if let Some(id) = self.lookup(root) {
                                if matches!(
                                    name.as_str(),
                                    "set" | "push_string" | "write" | "finish" | "put" | "bump"
                                ) || self.helper_mutates_param0.contains(name)
                                {
                                    modes.insert(id, CaptureMode::Mutate);
                                }
                            }
                        }
                    }
                }
                self.scan_closure_expr(callee, params, locals, modes);
                for a in args {
                    self.scan_closure_expr(&a.value, params, locals, modes);
                }
            }
            Expr::Field { base, .. }
            | Expr::Unary { operand: base, .. }
            | Expr::AddrOf { operand: base, .. }
            | Expr::Try { operand: base, .. }
            | Expr::Receive { chan: base, .. }
            | Expr::Paren { inner: base, .. } => {
                self.scan_closure_expr(base, params, locals, modes);
            }
            Expr::Index { base, index, .. } => {
                self.scan_closure_expr(base, params, locals, modes);
                self.scan_closure_expr(index, params, locals, modes);
            }
            Expr::Binary { lhs, rhs, .. } => {
                self.scan_closure_expr(lhs, params, locals, modes);
                self.scan_closure_expr(rhs, params, locals, modes);
            }
            _ => {}
        }
    }

    fn as_borrow_view_call(&self, expr: &Expr) -> Option<BindingId> {
        let inner = match expr {
            Expr::Try { operand, .. } => operand.as_ref(),
            other => other,
        };
        if let Expr::Call { callee, .. } = inner {
            if let Expr::Field { base, name, .. } = callee.as_ref() {
                if name == "array" {
                    if let Some(root) = base.root_ident() {
                        return self.lookup(root);
                    }
                }
            }
        }
        None
    }

    fn is_own_handle_producer(&self, expr: &Expr) -> bool {
        let inner = match expr {
            Expr::Try { operand, .. } => operand.as_ref(),
            other => other,
        };
        if let Expr::Call { callee, .. } = inner {
            if let Expr::Field { base, name, .. } = callee.as_ref() {
                if let Expr::Ident { name: pkg, .. } = base.as_ref() {
                    return (pkg == "serde_json"
                        && matches!(name.as_str(), "parse" | "new_object" | "new_array"))
                        || (pkg == "flate2" && name == "new_writer");
                }
            }
        }
        false
    }
}

// ===========================================================================
// Pre-Registered Test 5 Support — Codebase Coupling & LoC Honesty Census
// ===========================================================================

#[derive(Debug, Default, Clone)]
pub struct CouplingCensus {
    pub files_scanned: usize,
    pub expr_pattern_sites: usize,
    pub expr_paren_arms: usize,
    pub unwrap_parens_calls: usize,
    pub closure_sites: usize,
    pub spike_stage1_loc: usize,
    pub spike_stage2_loc: usize,
    pub spike_stage3_loc: usize,
    pub spike_lib_loc: usize,
}

pub fn measure_hy_check_coupling(hy_check_src: &Path, spike_lib_rs: &Path) -> CouplingCensus {
    let mut census = CouplingCensus::default();
    let entries = std::fs::read_dir(hy_check_src).expect("read hy-check/src");
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("rs") {
            continue;
        }
        census.files_scanned += 1;
        let text = std::fs::read_to_string(&path).expect("read rs");
        let prod_text = text.split("#[cfg(test)]").next().unwrap_or(&text);
        for line in prod_text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") {
                continue;
            }
            census.expr_pattern_sites += trimmed.matches("Expr::").count();
            if trimmed.contains("Expr::Paren") {
                census.expr_paren_arms += 1;
            }
            if trimmed.contains("unwrap_parens") {
                census.unwrap_parens_calls += 1;
            }
            if trimmed.contains("FnLit") || trimmed.contains("closure") {
                census.closure_sites += 1;
            }
        }
    }
    let spike_text = std::fs::read_to_string(spike_lib_rs).expect("read spike lib.rs");
    let mut section = 0usize; // 0 = header/imports, 1 = Stage 1, 2 = Stage 2, 3 = Stage 3, 4 = census
    for l in spike_text.lines() {
        let t = l.trim();
        if t.starts_with("// Stage 1 —") {
            section = 1;
        } else if t.starts_with("// Stage 2 —") {
            section = 2;
        } else if t.starts_with("fn summarize_closure(") {
            section = 3;
        } else if t.starts_with("// Pre-Registered Test 5 Support") {
            section = 4;
        }
        if !t.is_empty() && !t.starts_with("//") {
            census.spike_lib_loc += 1;
            match section {
                1 => census.spike_stage1_loc += 1,
                2 => census.spike_stage2_loc += 1,
                3 => census.spike_stage3_loc += 1,
                _ => {}
            }
        }
    }
    census
}
