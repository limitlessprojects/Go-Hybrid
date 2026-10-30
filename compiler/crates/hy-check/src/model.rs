//! model.rs — the Tier-0 checker's data model (SPEC-LANGUAGE.md §9).
//!
//! This is a deliberately LIGHTWEIGHT classification (per the task brief:
//! "a lightweight classification — NOT full type inference"): every binding
//! gets a `TypeShape` good enough to decide the §9 rules. There is no type
//! variable, no unification, no coercion — just enough structure to answer
//! "is this value sendable?" (O4) and "which loan rules apply?" (O7/O8/O12).

use hy_diag::{Diagnostic, ManagedBlockAudit, Span};
use hy_syntax::{Expr, File, FnDecl, TypeExpr};
use std::collections::{HashMap, HashSet};

// ---------------------------------------------------------------------------
// Type shapes
// ---------------------------------------------------------------------------

/// A coarse classification of a binding's type. `Unknown` is FAIL-CLOSED at
/// spawn/send boundaries (O4.9, human directive 2026-10-13): a value the
/// checker cannot prove safe may not silently cross a concurrency boundary —
/// it is rejected (HYB-E3005, rule FC) unless the site sits in an explicit,
/// auditable `managed` block. UNKNOWN LOCALS remain legal inside a function
/// (Tier 0 has no whole-program type system; unknown locals simply mean
/// "no sendability opinion unless a boundary is involved").
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TypeShape {
    /// `int`, `bool`, `float64`, ... — value copies (class 1).
    Primitive,
    /// `string` — immutable (class 4).
    Str,
    /// `atomic.*` — internally synchronized (class 2).
    Atomic,
    /// any `chan` direction (class 2); carries the element shape for
    /// loop-variable and receive inference.
    Chan(Box<TypeShape>),
    /// `map[K]V` — mutable GC composite (NOT sendable).
    Map(Box<TypeShape>, Box<TypeShape>),
    /// `[]T` — mutable GC composite (NOT sendable).
    Slice(Box<TypeShape>),
    /// `[N]T` — mutable GC composite (NOT sendable as a binding).
    Array(Box<TypeShape>),
    /// a struct (local decl name if known) — mutable GC composite.
    Struct(Option<String>),
    /// `*T` — pointer; sendable only when the pointee is atomic (OQ6).
    Pointer(Box<TypeShape>),
    /// an L3 own handle, e.g. `serde_json.value`, `flate2.writer` (O13).
    OwnHandle(String),
    /// an L3 borrow handle / element view, e.g. `serde_json.view` (O7).
    BorrowHandle(String),
    /// a Go-side opaque foreign type (`sync.mutex`, `http.*`, ...) — the A5
    /// perimeter; no loan discipline, not sendable at spawn/send.
    ForeignOpaque(String),
    /// a closure / fn-typed value (captures its environment — never
    /// sendable at spawn/send in Tier 0; A5 perimeter).
    Closure,
    /// unresolved / not understood — fail-closed at boundaries (O4.9).
    Unknown,
}

impl TypeShape {
    /// Class 1/2/4-by-type: primitives, strings, atomics, and channels (a
    /// channel is safe to share BY TYPE regardless of its element type — the
    /// value crossing the site is the channel itself). `Unknown` is
    /// deliberately ABSENT: an unproven shape must not pass as safe (the
    /// O4.9 fail-closed boundary — HYB-E3005).
    pub(crate) fn is_sendable_by_type(&self) -> bool {
        match self {
            TypeShape::Primitive | TypeShape::Str | TypeShape::Atomic | TypeShape::Chan(_) => true,
            // `&atomic_binding` is the mediated share route (OQ6); a pointer
            // to anything else is a mutable pointee — strict reject.
            TypeShape::Pointer(inner) => matches!(**inner, TypeShape::Atomic),
            _ => false,
        }
    }

    /// An L3 boundary handle (loan rules O7/O11 apply).
    pub(crate) fn is_handle(&self) -> bool {
        matches!(self, TypeShape::OwnHandle(_) | TypeShape::BorrowHandle(_))
    }

    /// Go reference semantics: assigning `b := a` makes `b` an ALIAS of `a`
    /// (maps and slices are reference types; pointers alias the pointee).
    /// Structs and arrays are copied by value in Go, so they are NOT
    /// binding-level aliases. Used by the freeze rule (O5.2).
    pub(crate) fn is_reference_alias(&self) -> bool {
        matches!(
            self,
            TypeShape::Map(_, _) | TypeShape::Slice(_) | TypeShape::Pointer(_)
        )
    }

    /// Short human name for diagnostics.
    pub(crate) fn noun(&self) -> &'static str {
        match self {
            TypeShape::Map(_, _) => "map",
            TypeShape::Slice(_) => "slice",
            TypeShape::Array(_) => "array",
            TypeShape::Struct(_) => "struct",
            TypeShape::Pointer(_) => "pointer",
            TypeShape::Chan(_) => "channel",
            TypeShape::Atomic => "atomic value",
            TypeShape::Str => "string",
            TypeShape::Primitive => "primitive value",
            TypeShape::OwnHandle(_) | TypeShape::BorrowHandle(_) => "boundary handle",
            TypeShape::ForeignOpaque(_) => "foreign value",
            TypeShape::Closure => "closure",
            TypeShape::Unknown => "value",
        }
    }

    /// Fallback rendering for diagnostics when no source `TypeExpr` is
    /// available (never panics).
    pub(crate) fn render(&self) -> String {
        match self {
            TypeShape::Primitive => "primitive".to_string(),
            TypeShape::Str => "string".to_string(),
            TypeShape::Atomic => "atomic".to_string(),
            TypeShape::Chan(e) => format!("chan {}", e.render()),
            TypeShape::Map(k, v) => format!("map[{}]{}", k.render(), v.render()),
            TypeShape::Slice(e) => format!("[]{}", e.render()),
            TypeShape::Array(e) => format!("[...]{}", e.render()),
            TypeShape::Struct(Some(n)) => n.clone(),
            TypeShape::Struct(None) => "struct".to_string(),
            TypeShape::Pointer(t) => format!("*{}", t.render()),
            TypeShape::OwnHandle(t) | TypeShape::BorrowHandle(t) | TypeShape::ForeignOpaque(t) => {
                t.clone()
            }
            TypeShape::Closure => "fn".to_string(),
            TypeShape::Unknown => "value of unresolvable type".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Bindings, moves, loans
// ---------------------------------------------------------------------------

/// Everything the checker knows about one local binding.
#[derive(Debug, Clone)]
pub(crate) struct Binding {
    pub shape: TypeShape,
    /// The source type as written (for diagnostics: "map[string]int").
    pub ty: Option<TypeExpr>,
    /// `let`/`const` binding or a file-level `const` (class 4).
    pub immutable: bool,
    /// `let` binding (T22: shallow immutable binding — no rebinding/reassignment).
    pub is_let: bool,
    /// Born inside a `managed` block (O6.2 taint): sendable everywhere,
    /// outside the §9.2 guarantee for its whole lifetime.
    pub managed_taint: bool,
    /// Moved-from / consumed-from state (O5.1 / O13).
    pub moved: Option<MoveInfo>,
    /// When `Some(reason)`, this binding is not exclusively owned by the
    /// current function/closure invocation (parameter, receiver, closure
    /// capture, or non-channel `for ... in` element), so `move x` at a
    /// concurrency boundary cannot freeze all live aliases (O5.1/O5.2/O5.4).
    pub non_owned_origin: Option<String>,
    /// Defined (or re-assigned) as an O8 map-slot loan `x := &m[k]`.
    /// T24: a loan is not an addressable value — `&x` on it is
    /// HYB-E3104. A shadowing redefinition creates a fresh binding with
    /// this flag off, so the rule stays scope-correct.
    pub slot_loan: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MoveKind {
    /// `move x` at a spawn/send/assignment/return (O5.1) — A1b.
    Moved,
    /// A consuming API call took ownership (O13) — A7.
    Consumed,
}

#[derive(Debug, Clone)]
pub(crate) struct MoveInfo {
    /// The transfer site (where ownership left).
    pub span: Span,
    pub kind: MoveKind,
    /// Self-describing site description, e.g.
    /// "the move-send `results <- move agg`" or "the consuming call `enc.finish()`".
    pub desc: String,
    /// The binding whose ownership actually left. Diagnostics name THIS, not
    /// the binding that happened to notice the use: a may-share peer (a
    /// reference-typed field or element read out earlier) is not in the
    /// single-parent `aliases` chain, so resolving the USED binding's root
    /// named the peer itself and produced a self-alias note for a move of a
    /// different binding.
    pub moved_name: String,
}

/// A live loan. `last` is the inclusive end of the non-lexical window, in
/// statement indices (see walk.rs for the indexing scheme).
#[derive(Debug, Clone)]
pub(crate) enum LoanTarget {
    /// O7/A2: borrow-mode loan on a boundary handle binding.
    Handle { handle: String },
    /// O8/A3: unique loan on one map slot; `binding` is the defined name
    /// (for last-use computation), `path` is the base map path ("e.services"),
    /// `key` the literal index when it provably names one slot (see
    /// `walk::addr_of_index_slot_key`); `None` may alias any slot of `path`.
    MapSlot {
        binding: String,
        path: String,
        key: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct Loan {
    pub target: LoanTarget,
    pub created: usize,
    pub last: usize,
    pub span: Span,
    pub desc: String,
}

/// Per-function mutable state (environment, aliases, live loans).
#[derive(Debug, Default, Clone)]
pub(crate) struct FnState {
    /// The function's BASE scope: receiver, params, and top-level defines.
    pub env: HashMap<String, Binding>,
    /// Saved alias maps for outer scopes when entering block scopes so an
    /// inner shadowing define (`if true { m2 := 1 }`) cannot wipe outer
    /// alias links (`m2 -> m`).
    pub alias_scopes: Vec<HashMap<String, String>>,
    /// Same discipline for the may-share edges (the seam's second freeze
    /// map, pr-35): a shadowing define's `share_clear` must not survive the
    /// block either — the snapshot restores at pop.
    pub share_scopes: Vec<HashMap<String, HashSet<String>>>,
    /// Current binding -> set of shadowed outer bindings `(outer_name, depth)`
    /// (`depth == 0` for `env`, `depth == i + 1` for `scopes[i]`) whose backing
    /// storage this binding may share. When the binding (or any peer in its
    /// alias/may-share component) is moved, `mark_move` immediately marks the
    /// shadowed outer binding at `depth` as moved.
    pub shadow_links: HashMap<String, HashSet<(String, usize)>>,
    pub shadow_link_scopes: Vec<HashMap<String, HashSet<(String, usize)>>>,
    /// Block scopes, innermost last (issue #14): a define inside a nested
    /// block lands in the top layer and DIES at block exit, so an inner
    /// `m := 1` can no longer overwrite the outer `m`'s binding and launder
    /// a shared map past a spawn site. Lookup resolves innermost-first.
    pub scopes: Vec<HashMap<String, Binding>>,
    /// alias name -> target binding name (binding-level only — enough for the
    /// 20-program corpus; documented limitation).
    pub aliases: HashMap<String, String>,
    /// undirected may-share graph (#27): binding name -> names that may share
    /// the same mutable GC storage. A Go assign `b := a` is a true alias only
    /// for reference types, but a SHALLOW copy of a struct/array that carries
    /// a reference field, a re-slice `s[:]`, a field/index projection, or a
    /// composite literal that embeds a live binding all leave `b` sharing
    /// storage with `a` too. The freeze rule (O5.2) must cover the whole
    /// connected component, not just the single-parent `aliases` chain.
    pub may_share: HashMap<String, HashSet<String>>,
    pub loans: Vec<Loan>,
    /// name -> last statement index that references it (Pass A result).
    pub last_use: HashMap<String, usize>,
    /// loop header index -> inclusive last statement index of that loop body.
    pub loop_end: HashMap<usize, usize>,
    /// Snapshot of active block-scoped bindings, aliases, and may_share sets
    /// captured when a block containing a closure literal (`FnLit`) exits.
    pub closure_scoped_envs: HashMap<
        (usize, usize),
        (
            HashMap<String, Binding>,
            HashMap<String, String>,
            HashMap<String, HashSet<String>>,
        ),
    >,
    /// Per-scope list of `FnLit` start `(line, col)` positions declared in
    /// that scope (used to snapshot block state right before `scope_pop`).
    pub fnlit_scopes: Vec<Vec<(usize, usize)>>,
    /// #111 form 4: a local closure binding -> the base paths its body may
    /// MUTATE (map insert/delete targets, transitively via calls it makes).
    /// Captured roots only (the closure's own params are excluded — they
    /// are fresh bindings, not the caller's storage). Recorded at define/
    /// rebind time; consumed at the CALL site so an insert inside the body
    /// counts as an A3 event when the call sits in a live slot-loan window.
    pub closure_mutates: HashMap<String, HashSet<String>>,
    /// Local closure binding -> `(params, named_results, body)` for
    /// interprocedural return-origin and multi-value return sharing analysis.
    pub closure_fnlits: HashMap<
        String,
        (
            Vec<(String, Option<hy_syntax::TypeExpr>)>,
            Vec<(String, Option<hy_syntax::TypeExpr>)>,
            hy_syntax::Block,
        ),
    >,
    /// Declared result shapes of the function being checked (rule N1's
    /// return-site nil checks). Empty for closure bodies (fn literals
    /// carry no result types on the AST surface yet).
    pub result_shapes: Vec<TypeShape>,
    /// Declared result types of the function being checked (for constant overflow checks).
    pub result_types: Vec<hy_syntax::TypeExpr>,
    /// Declared named result bindings (`fn f() (out map[string]int)`), in
    /// signature order. A bare `return` reads and returns each named result.
    pub named_results: Vec<String>,
    /// Bindings currently holding a function-local `&...` borrow (or a
    /// composite literal containing `&...`), for O2 `LOAN-ESCAPE` on
    /// named-result / variable returns (`out = &x; return` / `return out`).
    pub borrow_bindings: HashSet<String>,
    /// Local closure bindings -> `(params, results, body)` recorded at define/rebind
    /// time so call sites can analyze parameter/capture stores (`record_callee_param_stores`).
    pub closure_defs:
        HashMap<String, (Vec<hy_syntax::Param>, Vec<hy_syntax::FnResult>, hy_syntax::Block)>,
    pub closure_mutate_scopes: Vec<HashMap<String, HashSet<String>>>,
    pub closure_fnlit_scopes: Vec<
        HashMap<
            String,
            (
                Vec<(String, Option<hy_syntax::TypeExpr>)>,
                Vec<(String, Option<hy_syntax::TypeExpr>)>,
                hy_syntax::Block,
            ),
        >,
    >,
    pub closure_def_scopes: Vec<
        HashMap<String, (Vec<hy_syntax::Param>, Vec<hy_syntax::FnResult>, hy_syntax::Block)>,
    >,
    pub block_closure_mutates: HashMap<String, HashSet<String>>,
    pub block_closure_fnlits: HashMap<
        String,
        (
            Vec<(String, Option<hy_syntax::TypeExpr>)>,
            Vec<(String, Option<hy_syntax::TypeExpr>)>,
            hy_syntax::Block,
        ),
    >,
    pub block_closure_defs:
        HashMap<String, (Vec<hy_syntax::Param>, Vec<hy_syntax::FnResult>, hy_syntax::Block)>,
}

pub(crate) type ClosureDefEntry = (
    Vec<hy_syntax::Param>,
    Vec<hy_syntax::FnResult>,
    hy_syntax::Block,
);
pub(crate) type ClosureDef = ClosureDefEntry;

pub(crate) type ClosureFnLitEntry = (
    Vec<(String, Option<hy_syntax::TypeExpr>)>,
    Vec<(String, Option<hy_syntax::TypeExpr>)>,
    hy_syntax::Block,
);
pub(crate) type ClosureFnLit = ClosureFnLitEntry;

pub(crate) fn merge_closure_def(
    existing: &ClosureDefEntry,
    incoming: &ClosureDefEntry,
) -> ClosureDefEntry {
    let (params1, results1, body1) = existing;
    let (params2, results2, body2) = incoming;
    if body1.span == body2.span && body1.stmts.len() == body2.stmts.len() {
        return existing.clone();
    }
    let params = if params1.len() >= params2.len() {
        params1.clone()
    } else {
        params2.clone()
    };
    let mut results = results1.clone();
    for r2 in results2 {
        if let Some(n2) = &r2.name {
            if !results.iter().any(|r1| r1.name.as_ref() == Some(n2)) {
                results.push(r2.clone());
            }
        } else if results.is_empty() {
            results.push(r2.clone());
        }
    }
    let mut stmts2 = Vec::new();
    for (p1, p2) in params.iter().zip(params2.iter()) {
        if p1.name != p2.name && p1.name != "_" && p2.name != "_" {
            stmts2.push(hy_syntax::Stmt::Local(hy_syntax::LocalDecl {
                kind: hy_syntax::LocalKind::Define,
                names: vec![p2.name.clone()],
                ty: Some(p2.ty.clone()),
                values: vec![hy_syntax::Expr::Ident {
                    name: p1.name.clone(),
                    span: body2.span,
                }],
                span: body2.span,
            }));
        }
    }
    stmts2.extend(body2.stmts.iter().cloned());
    let block2 = hy_syntax::Block {
        stmts: stmts2,
        span: body2.span,
    };
    let merged_body = hy_syntax::Block {
        stmts: vec![
            hy_syntax::Stmt::Block {
                block: body1.clone(),
                span: body1.span,
            },
            hy_syntax::Stmt::Block {
                block: block2,
                span: body2.span,
            },
        ],
        span: body1.span,
    };
    (params, results, merged_body)
}

pub(crate) fn merge_closure_fnlit(
    existing: &ClosureFnLitEntry,
    incoming: &ClosureFnLitEntry,
) -> ClosureFnLitEntry {
    let (params1, results1, body1) = existing;
    let (params2, results2, body2) = incoming;
    if body1.span == body2.span && body1.stmts.len() == body2.stmts.len() {
        return existing.clone();
    }
    let params = if params1.len() >= params2.len() {
        params1.clone()
    } else {
        params2.clone()
    };
    let mut results = results1.clone();
    for (n2, ty2) in results2 {
        if !results.iter().any(|(n1, _)| n1 == n2) {
            results.push((n2.clone(), ty2.clone()));
        }
    }
    let mut stmts2 = Vec::new();
    for ((p1, _), (p2, ty2)) in params.iter().zip(params2.iter()) {
        if p1 != p2 && p1 != "_" && p2 != "_" {
            stmts2.push(hy_syntax::Stmt::Local(hy_syntax::LocalDecl {
                kind: hy_syntax::LocalKind::Define,
                names: vec![p2.clone()],
                ty: ty2.clone(),
                values: vec![hy_syntax::Expr::Ident {
                    name: p1.clone(),
                    span: body2.span,
                }],
                span: body2.span,
            }));
        }
    }
    stmts2.extend(body2.stmts.iter().cloned());
    let block2 = hy_syntax::Block {
        stmts: stmts2,
        span: body2.span,
    };
    let merged_body = hy_syntax::Block {
        stmts: vec![
            hy_syntax::Stmt::Block {
                block: body1.clone(),
                span: body1.span,
            },
            hy_syntax::Stmt::Block {
                block: block2,
                span: body2.span,
            },
        ],
        span: body1.span,
    };
    (params, results, merged_body)
}

impl FnState {
    /// Enter a block scope (walk.rs's Push events).
    pub(crate) fn scope_push(&mut self) {
        self.scopes.push(HashMap::new());
        self.alias_scopes.push(self.aliases.clone());
        self.share_scopes.push(self.may_share.clone());
        self.shadow_link_scopes.push(self.shadow_links.clone());
        self.closure_mutate_scopes.push(self.closure_mutates.clone());
        self.closure_fnlit_scopes.push(self.closure_fnlits.clone());
        self.closure_def_scopes.push(self.closure_defs.clone());
        self.fnlit_scopes.push(Vec::new());
    }

    /// Exit the innermost block scope (walk.rs's Pop events).
    pub(crate) fn scope_pop(&mut self) {
        if let Some(fnlits) = self.fnlit_scopes.pop() {
            if !fnlits.is_empty() {
                let mut snap_env = self.env.clone();
                for layer in &self.scopes {
                    for (k, v) in layer {
                        snap_env.insert(k.clone(), v.clone());
                    }
                }
                let snap_aliases = self.aliases.clone();
                let snap_share = self.may_share.clone();
                for pos in fnlits {
                    self.closure_scoped_envs.entry(pos).or_insert_with(|| {
                        (snap_env.clone(), snap_aliases.clone(), snap_share.clone())
                    });
                }
            }
        }
        let popped_scope = self.scopes.last().cloned().unwrap_or_default();
        let exists_in_outer = |name: &str, env: &HashMap<String, Binding>, scopes: &[HashMap<String, Binding>]| {
            env.contains_key(name) || scopes.iter().rev().skip(1).any(|s| s.contains_key(name))
        };
        let survives_as_outer = |r: &str,
                                 aliases: &HashMap<String, String>,
                                 env: &HashMap<String, Binding>,
                                 scopes: &[HashMap<String, Binding>]| {
            !popped_scope.contains_key(r)
                || (aliases.get(r).map(|t| t == r).unwrap_or(false)
                    && exists_in_outer(r, env, scopes))
        };
        let to_outer_name = |name: &str,
                             aliases: &HashMap<String, String>,
                             env: &HashMap<String, Binding>,
                             scopes: &[HashMap<String, Binding>]|
         -> Option<String> {
            if survives_as_outer(name, aliases, env, scopes) {
                Some(name.to_string())
            } else {
                let r = crate::freeze::resolve_alias(self, name);
                if survives_as_outer(&r, aliases, env, scopes) {
                    Some(r)
                } else {
                    None
                }
            }
        };

        // Propagate non_owned_origin from popped-scope bindings (e.g. range vars,
        // match arm bindings, self-shadowed params) to any surviving outer binding
        // that joined their sharing group or alias chain inside the block.
        let mut inner_outer_origins: Vec<(String, String)> = Vec::new();
        for (p, pb) in &popped_scope {
            if let Some(orig) = &pb.non_owned_origin {
                let peer_orig = if orig.starts_with("a may-share peer of ") {
                    orig.clone()
                } else {
                    format!("a may-share peer of {}", orig)
                };
                let mut group = self.share_group(p);
                for a in self.aliases.keys() {
                    if crate::freeze::resolve_alias(self, a) == *p {
                        group.insert(a.clone());
                    }
                }
                for a in group {
                    if &a == p {
                        continue;
                    }
                    if let Some(oa) = to_outer_name(&a, &self.aliases, &self.env, &self.scopes) {
                        let direct_alias = crate::freeze::resolve_alias(self, &a) == *p;
                        inner_outer_origins.push((
                            oa,
                            if direct_alias {
                                orig.clone()
                            } else {
                                peer_orig.clone()
                            },
                        ));
                    }
                }
            }
        }
        // Capture any moves on inner aliases or may-share peers before the
        // inner scope and alias/share maps are popped (handles the case where
        // an inner alias/holder was moved while an outer peer was shadowed).
        let mut inner_root_moves: Vec<(String, MoveInfo)> = Vec::new();
        for (a, target) in &self.aliases {
            let root = crate::freeze::resolve_alias(self, target);
            if survives_as_outer(&root, &self.aliases, &self.env, &self.scopes) {
                if let Some(mv) = self.env_lookup(a).and_then(|b| b.moved.clone()) {
                    inner_root_moves.push((root, mv));
                }
            }
        }
        for a in self.may_share.keys() {
            if let Some(mv) = self.env_lookup(a).and_then(|b| b.moved.clone()) {
                for peer in self.share_group(a) {
                    let root = crate::freeze::resolve_alias(self, &peer);
                    if survives_as_outer(&peer, &self.aliases, &self.env, &self.scopes) {
                        inner_root_moves.push((peer, mv.clone()));
                    }
                    if survives_as_outer(&root, &self.aliases, &self.env, &self.scopes) {
                        inner_root_moves.push((root, mv.clone()));
                    }
                }
            }
        }
        // Preserve may-share relationships established inside the block between
        // outer bindings (including through self-shadowed outer bindings).
        let mut inner_outer_shares: Vec<(String, String)> = Vec::new();
        for a in self.may_share.keys() {
            let Some(oa) = to_outer_name(a, &self.aliases, &self.env, &self.scopes) else {
                continue;
            };
            for b in self.share_group(a) {
                if let Some(ob) = to_outer_name(&b, &self.aliases, &self.env, &self.scopes) {
                    inner_outer_shares.push((oa.clone(), ob));
                }
            }
        }
        self.scopes.pop();
        if let Some(outer_shadows) = self.shadow_link_scopes.pop() {
            self.shadow_links = outer_shadows;
        }
        if let Some(outer_aliases) = self.alias_scopes.pop() {
            self.aliases = outer_aliases;
            if let Some(outer_share) = self.share_scopes.pop() {
                self.may_share = outer_share;
                for (a, b) in inner_outer_shares {
                    self.share_edge(&a, &b);
                }
            }
            for (oa, orig) in inner_outer_origins {
                if let Some(bo) = self.env_lookup_mut(&oa) {
                    if bo.non_owned_origin.is_none() {
                        bo.non_owned_origin = Some(orig);
                    }
                }
            }
            for (root, mv) in inner_root_moves {
                if let Some(br) = self.env_lookup_mut(&root) {
                    if br.moved.is_none() {
                        br.moved = Some(mv.clone());
                    }
                }
                for peer in self.share_group(&root) {
                    if let Some(bp) = self.env_lookup_mut(&peer) {
                        if bp.moved.is_none() {
                            bp.moved = Some(mv.clone());
                        }
                    }
                }
            }
            // If an outer binding was moved while one of its outer aliases
            // was shadowed inside the block, propagate the freeze across the
            // restored outer alias group now that the shadow has died.
            let keys: Vec<String> = self.aliases.keys().cloned().collect();
            for a in keys {
                let root = crate::freeze::resolve_alias(self, &a);
                let moved = self
                    .env_lookup(&root)
                    .and_then(|b| b.moved.clone())
                    .or_else(|| self.env_lookup(&a).and_then(|b| b.moved.clone()));
                if let Some(mv) = moved {
                    if let Some(ba) = self.env_lookup_mut(&a) {
                        if ba.moved.is_none() {
                            ba.moved = Some(mv.clone());
                        }
                    }
                    if let Some(br) = self.env_lookup_mut(&root) {
                        if br.moved.is_none() {
                            br.moved = Some(mv);
                        }
                    }
                }
            }
            // Issue #195: also propagate freeze across the restored outer
            // `may_share` connected components (and their alias chains) so a
            // shadowed struct-embedded map or slice peer cannot escape the
            // move of its holder once the inner shadow scope exits.
            let share_keys: Vec<String> = self.may_share.keys().cloned().collect();
            for k in share_keys {
                let root = crate::freeze::resolve_alias(self, &k);
                let moved = self
                    .env_lookup(&k)
                    .and_then(|b| b.moved.clone())
                    .or_else(|| self.env_lookup(&root).and_then(|b| b.moved.clone()));
                if let Some(mv) = moved {
                    let mut group: Vec<String> = self.share_group(&k).into_iter().collect();
                    group.push(k.clone());
                    group.push(root);
                    let seeded = group.clone();
                    for member in &seeded {
                        let mroot = crate::freeze::resolve_alias(self, member);
                        group.push(mroot.clone());
                        for a in self.aliases.keys() {
                            if crate::freeze::resolve_alias(self, a) == mroot {
                                group.push(a.clone());
                            }
                        }
                    }
                    for peer in group {
                        if let Some(bp) = self.env_lookup_mut(&peer) {
                            if bp.moved.is_none() {
                                bp.moved = Some(mv.clone());
                            }
                        }
                    }
                }
            }
        }
        let key_in_popped = |k: &str| -> bool {
            let root = k.split('.').next().unwrap_or(k).trim_end_matches("[]");
            popped_scope.contains_key(k) || popped_scope.contains_key(root)
        };
        if let Some(outer_mutates) = self.closure_mutate_scopes.pop() {
            let popped_keys: Vec<String> = self
                .closure_mutates
                .keys()
                .filter(|k| key_in_popped(k))
                .cloned()
                .collect();
            for k in popped_keys {
                if let Some(curr) = self.closure_mutates.remove(&k) {
                    self.block_closure_mutates
                        .entry(k)
                        .or_default()
                        .extend(curr);
                }
            }
            for k in popped_scope.keys() {
                if let Some(prev) = outer_mutates.get(k) {
                    self.closure_mutates.insert(k.clone(), prev.clone());
                }
            }
            for (k, prev) in outer_mutates {
                if key_in_popped(&k) {
                    self.closure_mutates.insert(k, prev);
                } else {
                    self.closure_mutates.entry(k).or_default().extend(prev);
                }
            }
        }
        if let Some(outer_fnlits) = self.closure_fnlit_scopes.pop() {
            let popped_keys: Vec<String> = self
                .closure_fnlits
                .keys()
                .filter(|k| key_in_popped(k))
                .cloned()
                .collect();
            for k in popped_keys {
                if let Some(curr) = self.closure_fnlits.remove(&k) {
                    let merged = match self.block_closure_fnlits.get(&k) {
                        Some(prev) => merge_closure_fnlit(prev, &curr),
                        None => curr,
                    };
                    self.block_closure_fnlits.insert(k, merged);
                }
            }
            for (k, prev) in outer_fnlits {
                if key_in_popped(&k) {
                    self.closure_fnlits.insert(k, prev);
                } else {
                    match self.closure_fnlits.get(&k) {
                        Some(curr) => {
                            let merged = merge_closure_fnlit(&prev, curr);
                            self.closure_fnlits.insert(k, merged);
                        }
                        None => {
                            self.closure_fnlits.insert(k, prev);
                        }
                    }
                }
            }
        }
        if let Some(outer_defs) = self.closure_def_scopes.pop() {
            let popped_keys: Vec<String> = self
                .closure_defs
                .keys()
                .filter(|k| key_in_popped(k))
                .cloned()
                .collect();
            for k in popped_keys {
                if let Some(curr) = self.closure_defs.remove(&k) {
                    let merged = match self.block_closure_defs.get(&k) {
                        Some(prev) => merge_closure_def(prev, &curr),
                        None => curr,
                    };
                    self.block_closure_defs.insert(k, merged);
                }
            }
            for (k, prev) in outer_defs {
                if key_in_popped(&k) {
                    self.closure_defs.insert(k, prev);
                } else {
                    match self.closure_defs.get(&k) {
                        Some(curr) => {
                            let merged = merge_closure_def(&prev, curr);
                            self.closure_defs.insert(k, merged);
                        }
                        None => {
                            self.closure_defs.insert(k, prev);
                        }
                    }
                }
            }
        }
    }

    /// Return the scope depth of the currently visible binding `name`:
    /// `Some(0)` for `self.env`, `Some(i + 1)` for `self.scopes[i]`.
    pub(crate) fn lookup_depth(&self, name: &str) -> Option<usize> {
        for (i, layer) in self.scopes.iter().enumerate().rev() {
            if layer.contains_key(name) {
                return Some(i + 1);
            }
        }
        if self.env.contains_key(name) {
            Some(0)
        } else {
            None
        }
    }

    /// Mutable lookup of a binding at an explicit scope depth (`0` for `env`,
    /// `i + 1` for `scopes[i]`).
    pub(crate) fn binding_at_depth_mut(&mut self, name: &str, depth: usize) -> Option<&mut Binding> {
        if depth == 0 {
            self.env.get_mut(name)
        } else {
            self.scopes.get_mut(depth - 1).and_then(|s| s.get_mut(name))
        }
    }

    /// Innermost-first binding lookup (scopes, then the base env).
    pub(crate) fn env_lookup(&self, name: &str) -> Option<&Binding> {
        for layer in self.scopes.iter().rev() {
            if let Some(b) = layer.get(name) {
                return Some(b);
            }
        }
        self.env.get(name)
    }

    /// Mutable lookup, same resolution order (moves, rebinds, freezes must
    /// hit the binding a use at this depth actually refers to).
    pub(crate) fn env_lookup_mut(&mut self, name: &str) -> Option<&mut Binding> {
        for layer in self.scopes.iter_mut().rev() {
            if layer.contains_key(name) {
                return layer.get_mut(name);
            }
        }
        self.env.get_mut(name)
    }

    /// Define a binding at the CURRENT depth: the top block scope when one
    /// is open, else the base env. A shadowed outer entry stays intact in
    /// its own layer — the fix for issue #14 (a flat env let the inner
    /// define overwrite the outer map's shape and launder it past a spawn).
    pub(crate) fn env_define(&mut self, name: &str, b: Binding) {
        if let Some(top) = self.scopes.last_mut() {
            top.insert(name.to_string(), b);
        } else {
            self.env.insert(name.to_string(), b);
        }
    }

    /// Record that `a` and `b` may share mutable GC storage (#27). Undirected:
    /// a `move` of either must freeze the other.
    pub(crate) fn share_edge(&mut self, a: &str, b: &str) {
        if a == b {
            return;
        }
        self.may_share
            .entry(a.to_string())
            .or_default()
            .insert(b.to_string());
        self.may_share
            .entry(b.to_string())
            .or_default()
            .insert(a.to_string());
        if let Some(a_links) = self.shadow_links.get(a).cloned() {
            self.shadow_links
                .entry(b.to_string())
                .or_default()
                .extend(a_links);
        }
        if let Some(b_links) = self.shadow_links.get(b).cloned() {
            self.shadow_links
                .entry(a.to_string())
                .or_default()
                .extend(b_links);
        }
    }

    /// Drop `name`'s may-share edges: a whole-binding rebind re-owns the
    /// binding, so it leaves the old sharing component while preserving
    /// transitive sharing links among its remaining peers.
    pub(crate) fn share_clear(&mut self, name: &str) {
        let neighbors: Vec<String> = self
            .may_share
            .get(name)
            .into_iter()
            .flatten()
            .cloned()
            .chain(
                self.aliases
                    .iter()
                    .filter_map(|(k, v)| if v == name { Some(k.clone()) } else { None })
                    .chain(self.aliases.get(name).cloned()),
            )
            .filter(|p| p != name)
            .collect();
        if let Some(links) = self.shadow_links.remove(name) {
            for nb in &neighbors {
                self.shadow_links
                    .entry(nb.clone())
                    .or_default()
                    .extend(links.iter().cloned());
            }
        }
        if let Some(peers) = self.may_share.remove(name) {
            let peer_list: Vec<String> = peers.into_iter().collect();
            for p in &peer_list {
                if let Some(set) = self.may_share.get_mut(p) {
                    set.remove(name);
                }
            }
            for i in 0..peer_list.len() {
                for j in (i + 1)..peer_list.len() {
                    self.share_edge(&peer_list[i], &peer_list[j]);
                }
            }
        }
    }

    /// The connected component of `name` under may-share, EXCLUDING `name`
    /// itself (bounded to bindings that exist in this environment).
    pub(crate) fn share_group(&self, name: &str) -> HashSet<String> {
        let mut seen: HashSet<String> = HashSet::new();
        let mut stack = vec![name.to_string()];
        while let Some(n) = stack.pop() {
            if let Some(peers) = self.may_share.get(&n) {
                for p in peers {
                    if seen.insert(p.clone()) {
                        stack.push(p.clone());
                    }
                }
            }
        }
        seen.remove(name);
        seen
    }
}

// ---------------------------------------------------------------------------
// The checker (file-level tables + accumulators)
// ---------------------------------------------------------------------------

pub(crate) struct Checker<'a> {
    pub file: &'a File,
    /// Source path, cloned onto every diagnostic.
    pub path: String,
    /// struct name -> (field name, field type) — file-level struct decls.
    pub structs: HashMap<String, Vec<(String, TypeExpr)>>,
    /// struct name -> declared type parameters (including empty-field structs, #722).
    pub struct_type_params: HashMap<String, Vec<hy_syntax::TypeParam>>,
    /// enum name -> declared variant names in source order (T13/T14 HYB-E5001).
    pub enums: HashMap<String, Vec<String>>,
    /// enum name -> whether the first variant is payload-free (N3 HYB-E4002).
    pub enum_has_zero_value: HashMap<String, bool>,
    /// type alias name -> target TypeExpr.
    pub type_aliases: HashMap<String, TypeExpr>,
    /// fn name -> decl (return-type inference for single-value defines).
    pub fns: HashMap<String, &'a FnDecl>,
    /// Issue #758: names defined inside the deep-walked closure body
    /// currently under the zero-divisor walk — a closure-local shadow
    /// is invisible to `st`, so resolution fails open on them.
    pub zd_defined: std::collections::HashSet<String>,
    /// (receiver base type name, method name) -> decl (method return-type inference).
    pub method_decls: HashMap<(String, String), &'a FnDecl>,
    /// file-level consts (immutable, visible in every fn).
    pub consts: HashMap<String, Binding>,
    /// File-level const literal values, for constant-folding identifier
    /// references (`const big = 300; var n int8 = big` must be rejected
    /// like the literal).
    pub const_values: HashMap<String, &'a Expr>,
    /// Known foreign package prefixes (imports ∪ the builtin table).
    pub packages: HashSet<String>,
    /// Package-scope names (issue #201-C): top-level fns / consts / structs
    /// share one namespace in Go — a second declaration of the same name is
    /// `X redeclared in this block`. Keyed by name, value = declaring kind.
    pub pkg_names: HashMap<String, &'static str>,
    /// Declared methods (issue #201-C): (receiver base type, method name) —
    /// methods live in the receiver's method set, not package scope, so
    /// `box.get` twice is an error while `fn get` + `box.get` is legal.
    pub methods: HashSet<(String, String)>,
    /// Emitted diagnostics (unsorted; check_file sorts by span).
    pub diags: Vec<Diagnostic>,
    // --report-managed counters (O6.3)
    pub managed_blocks: usize,
    pub sites_via_managed: usize,
    pub total_sites: usize,
    // Row 14 (discussion #4): every managed block's payload, for the audit.
    pub managed_audit: Vec<ManagedBlockAudit>,
    /// Names each enclosing closure body rebinds, innermost last — the
    /// emit-guard deep walk skips them (addressable.rs).
    pub closure_bound: Vec<HashSet<String>>,
}

/// Row 14 (discussion #4): the CLOSED category set — the invariant a
/// `managed` block deliberately bypasses, machine-checked so reviewers audit
/// by class instead of by prose. An unknown category is a reject (HYB-E0004).
pub const MANAGED_CATEGORIES: &[&str] = &[
    "mutex",    // shared state guarded by a lock
    "atomic",   // lock-free coordination via sync/atomic
    "actor",    // single-owner event loop / channel-owned state
    "finalizer",// Drop / runtime.SetFinalizer interplay
    "interop",  // cgo / unsafe / FFI edges
    "other",    // anything else — the reason must carry the specifics
];

impl<'a> Checker<'a> {
    pub(crate) fn new(file: &'a File) -> Self {
        Checker {
            file,
            path: file.path.clone(),
            structs: HashMap::new(),
            struct_type_params: HashMap::new(),
            enums: HashMap::new(),
            enum_has_zero_value: HashMap::new(),
            type_aliases: HashMap::new(),
            fns: HashMap::new(),
            zd_defined: std::collections::HashSet::new(),
            method_decls: HashMap::new(),
            consts: HashMap::new(),
            const_values: HashMap::new(),
            packages: HashSet::new(),
            pkg_names: HashMap::new(),
            methods: HashSet::new(),
            diags: Vec::new(),
            managed_blocks: 0,
            sites_via_managed: 0,
            total_sites: 0,
            managed_audit: Vec::new(),
            closure_bound: Vec::new(),
        }
    }

    /// Binding lookup: locals first (innermost scope wins), then file-level
    /// consts.
    pub(crate) fn lookup_binding<'b>(&'b self, st: &'b FnState, name: &str) -> Option<&'b Binding> {
        st.env_lookup(name).or_else(|| self.consts.get(name))
    }

    /// The type string for diagnostics: the source type when we have it,
    /// else the shape fallback.
    pub(crate) fn binding_ty_str(&self, b: &Binding) -> String {
        b.ty.as_ref()
            .map(|t| t.render())
            .unwrap_or_else(|| b.shape.render())
    }
}

// ---------------------------------------------------------------------------
// Small shared expression helpers
// ---------------------------------------------------------------------------

/// `&x` — both legal AST spellings (`Expr::AddrOf` and `Expr::Unary{Addr}`).
pub(crate) fn as_addr_of(e: &Expr) -> Option<&Expr> {
    match unwrap_parens(e) {
        Expr::AddrOf { operand, .. } => Some(unwrap_parens(operand)),
        Expr::Unary {
            op: hy_syntax::UnOp::Addr,
            operand,
            ..
        } => Some(unwrap_parens(operand)),
        _ => None,
    }
}

/// Strip enclosing parentheses from an expression.
pub(crate) fn unwrap_parens(mut e: &Expr) -> &Expr {
    while let Expr::Paren { inner, .. } = e {
        e = inner;
    }
    e
}

/// The direct operand of a `move x` (or `move (x)`) when it is a plain binding.
pub(crate) fn move_operand_ident(e: &Expr) -> Option<String> {
    match unwrap_parens(e) {
        Expr::Move { operand, .. } => match unwrap_parens(operand) {
            Expr::Ident { name, .. } => Some(name.clone()),
            // `move s.conn` (own-handle field moves, O5.4) are handled by
            // `collect_move_operands` + the O5.4 move-form check (HYB-E3004).
            _ => None,
        },
        _ => None,
    }
}

/// Collect the root binding of every reference-carrying sub-expression of
/// `e` — the generalized form of `Expr::root_ident()`.
///
/// `root_ident()` stops at anything that is not an ident/field/index/slice chain:
/// a CALL result (`same(same(m))`), a view of a call result (`same(m)[0:2]`),
/// a composite literal (`wrap{items: same(m)}`), a field read out of a
/// literal (`box{items: m}.items`) or a call that returns a parameter field
/// (`take(box{items: m})`) all yield `None`, so the may-share/alias edge that
/// keeps O5.2 honest was never recorded for them. This walks the wrappers
/// instead and reports every binding the value could still share storage
/// with. `move`-wrapped operands transfer ownership and are skipped.
pub(crate) fn collect_ref_roots(e: &Expr, out: &mut Vec<String>) {
    match unwrap_parens(e) {
        Expr::Move { .. } => {}
        Expr::Ident { name, .. } => {
            if name != "_" {
                out.push(name.clone());
            }
        }
        Expr::Try { operand, .. } => collect_ref_roots(operand, out),
        Expr::Slice { base, .. } => collect_ref_roots(base, out),
        Expr::Field { base, .. } => collect_ref_roots(base, out),
        Expr::Index { base, .. } => collect_ref_roots(base, out),
        Expr::Call { callee, args, .. } => {
            for a in args {
                collect_ref_roots(&a.value, out);
            }
            // `x := r.method(...)` or `x := f(...)` where `f` is a closure
            // binding or expression holding a closure: the receiver or
            // captured environment can hand back interior storage. Top-level
            // function names are not in `st.env`/`self.consts`, so
            // `lookup_binding` ignores them.
            collect_ref_roots(callee, out);
        }
        Expr::FnLit {
            params,
            results,
            body,
            ..
        } => {
            let mut bound: HashSet<String> = params.iter().map(|p| p.name.clone()).collect();
            bound.extend(results.iter().filter_map(|r| r.name.clone()));
            crate::loans::collect_capture_idents_block(body, &mut bound, out);
        }
        Expr::StructLit { fields, .. } => {
            for f in fields {
                collect_ref_roots(&f.value, out);
            }
        }
        Expr::MapLit { entries, .. } => {
            for (k, v) in entries {
                collect_ref_roots(k, out);
                collect_ref_roots(v, out);
            }
        }
        Expr::SliceLit { items, .. } => {
            for it in items {
                collect_ref_roots(it, out);
            }
        }
        Expr::AddrOf { operand, .. } => collect_ref_roots(operand, out),
        Expr::Unary {
            op: hy_syntax::UnOp::Addr,
            operand,
            ..
        } => collect_ref_roots(operand, out),
        _ => {}
    }
}

/// Collect root identifiers that are explicitly borrowed via address-of (`&x`) inside composite literals
/// (e.g. `[]*T{&x}`, `map[K]*T{"k": &x}`, nested composite literals).
/// Ordinary calls, binary operations, indexing, and fields do not propagate borrow roots into results.
pub(crate) fn collect_addr_of_roots(e: &Expr, out: &mut Vec<String>) {
    match unwrap_parens(e) {
        Expr::AddrOf { operand, .. } => {
            if let Some(id) = operand.root_ident() {
                if id != "_" {
                    out.push(id.to_string());
                }
            }
            collect_addr_of_roots(operand, out);
        }
        Expr::Unary {
            op: hy_syntax::UnOp::Addr,
            operand,
            ..
        } => {
            if let Some(id) = operand.root_ident() {
                if id != "_" {
                    out.push(id.to_string());
                }
            }
            collect_addr_of_roots(operand, out);
        }
        Expr::Try { operand, .. } => collect_addr_of_roots(operand, out),
        Expr::Move { operand, .. } => collect_addr_of_roots(operand, out),
        Expr::StructLit { fields, .. } => {
            for f in fields {
                collect_addr_of_roots(&f.value, out);
            }
        }
        Expr::MapLit { entries, .. } => {
            for (k, v) in entries {
                collect_addr_of_roots(k, out);
                collect_addr_of_roots(v, out);
            }
        }
        Expr::SliceLit { items, .. } => {
            for it in items {
                collect_addr_of_roots(it, out);
            }
        }
        _ => {}
    }
}

/// Collect every `move <operand>` inside `e` (top-level or nested inside
/// composite literals / parens) so nested moves freeze their binding and
/// non-binding GC field moves trigger HYB-E3004 (O5.1 / O5.4).
pub(crate) fn collect_move_operands<'e>(e: &'e Expr, out: &mut Vec<(&'e Expr, Span)>) {
    match e {
        Expr::Move { operand, span } => {
            let inner = unwrap_parens(operand);
            out.push((inner, *span));
            if !matches!(inner, Expr::Ident { .. }) {
                collect_move_operands(inner, out);
            }
        }
        Expr::Paren { inner, .. } => collect_move_operands(inner, out),
        Expr::StructLit { fields, .. } => {
            for f in fields {
                collect_move_operands(&f.value, out);
            }
        }
        Expr::MapLit { entries, .. } => {
            for (k, v) in entries {
                collect_move_operands(k, out);
                collect_move_operands(v, out);
            }
        }
        Expr::SliceLit { items, .. } => {
            for item in items {
                collect_move_operands(item, out);
            }
        }
        Expr::Field { base, .. } => collect_move_operands(base, out),
        Expr::Index { base, index, .. } => {
            collect_move_operands(base, out);
            collect_move_operands(index, out);
        }
        Expr::Slice { base, lo, hi, .. } => {
            collect_move_operands(base, out);
            if let Some(l) = lo {
                collect_move_operands(l, out);
            }
            if let Some(h) = hi {
                collect_move_operands(h, out);
            }
        }
        Expr::Call { callee, args, .. } => {
            collect_move_operands(callee, out);
            for a in args {
                collect_move_operands(&a.value, out);
            }
        }
        Expr::Try { operand, .. } => collect_move_operands(operand, out),
        Expr::Unary { operand, .. } | Expr::AddrOf { operand, .. } => {
            collect_move_operands(operand, out);
        }
        Expr::Binary { lhs, rhs, .. } => {
            collect_move_operands(lhs, out);
            collect_move_operands(rhs, out);
        }
        Expr::Receive { chan, .. } => collect_move_operands(chan, out),
        Expr::Interp { parts, .. } => {
            for p in parts {
                if let hy_syntax::InterpPart::Expr(exp) = p {
                    collect_move_operands(exp, out);
                }
            }
        }
        _ => {}
    }
}

