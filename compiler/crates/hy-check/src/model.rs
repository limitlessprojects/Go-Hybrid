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
}

/// A live loan. `last` is the inclusive end of the non-lexical window, in
/// statement indices (see walk.rs for the indexing scheme).
#[derive(Debug, Clone)]
pub(crate) enum LoanTarget {
    /// O7/A2: borrow-mode loan on a boundary handle binding.
    Handle { handle: String },
    /// O8/A3: unique loan on one map slot; `binding` is the defined name
    /// (for last-use computation), `path` is the base map path ("e.services").
    MapSlot { binding: String, path: String },
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
    /// #111 form 4: a local closure binding -> the base paths its body may
    /// MUTATE (map insert/delete targets, transitively via calls it makes).
    /// Captured roots only (the closure's own params are excluded — they
    /// are fresh bindings, not the caller's storage). Recorded at define/
    /// rebind time; consumed at the CALL site so an insert inside the body
    /// counts as an A3 event when the call sits in a live slot-loan window.
    pub closure_mutates: HashMap<String, HashSet<String>>,
    /// Declared result shapes of the function being checked (rule N1's
    /// return-site nil checks). Empty for closure bodies (fn literals
    /// carry no result types on the AST surface yet).
    pub result_shapes: Vec<TypeShape>,
}

impl FnState {
    /// Enter a block scope (walk.rs's Push events).
    pub(crate) fn scope_push(&mut self) {
        self.scopes.push(HashMap::new());
        self.alias_scopes.push(self.aliases.clone());
        self.share_scopes.push(self.may_share.clone());
    }

    /// Exit the innermost block scope (walk.rs's Pop events).
    pub(crate) fn scope_pop(&mut self) {
        // Capture any moves on inner aliases or may-share peers before the
        // inner scope and alias/share maps are popped (handles the case where
        // an inner alias/holder was moved while an outer peer was shadowed).
        let mut inner_root_moves: Vec<(String, MoveInfo)> = Vec::new();
        for (a, target) in &self.aliases {
            let root = crate::freeze::resolve_alias(self, target);
            if let Some(mv) = self.env_lookup(a).and_then(|b| b.moved.clone()) {
                inner_root_moves.push((root, mv));
            }
        }
        for a in self.may_share.keys() {
            if let Some(mv) = self.env_lookup(a).and_then(|b| b.moved.clone()) {
                for peer in self.share_group(a) {
                    let root = crate::freeze::resolve_alias(self, &peer);
                    inner_root_moves.push((peer, mv.clone()));
                    inner_root_moves.push((root, mv.clone()));
                }
            }
        }
        self.scopes.pop();
        if let Some(outer_aliases) = self.alias_scopes.pop() {
            self.aliases = outer_aliases;
            if let Some(outer_share) = self.share_scopes.pop() {
                self.may_share = outer_share;
            }
            for (root, mv) in inner_root_moves {
                if let Some(br) = self.env_lookup_mut(&root) {
                    if br.moved.is_none() {
                        br.moved = Some(mv);
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
    }

    /// Drop `name`'s may-share edges: a whole-binding rebind re-owns the
    /// binding, so it leaves the old sharing component while preserving
    /// transitive sharing links among its remaining peers.
    pub(crate) fn share_clear(&mut self, name: &str) {
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
    /// enum name -> declared variant names in source order (T13/T14 HYB-E5001).
    pub enums: HashMap<String, Vec<String>>,
    /// enum name -> whether the first variant is payload-free (N3 HYB-E4002).
    pub enum_has_zero_value: HashMap<String, bool>,
    /// type alias name -> target TypeExpr.
    pub type_aliases: HashMap<String, TypeExpr>,
    /// fn name -> decl (return-type inference for single-value defines).
    pub fns: HashMap<String, &'a FnDecl>,
    /// (receiver base type name, method name) -> decl (method return-type inference).
    pub method_decls: HashMap<(String, String), &'a FnDecl>,
    /// file-level consts (immutable, visible in every fn).
    pub consts: HashMap<String, Binding>,
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
            enums: HashMap::new(),
            enum_has_zero_value: HashMap::new(),
            type_aliases: HashMap::new(),
            fns: HashMap::new(),
            method_decls: HashMap::new(),
            consts: HashMap::new(),
            packages: HashSet::new(),
            pkg_names: HashMap::new(),
            methods: HashSet::new(),
            diags: Vec::new(),
            managed_blocks: 0,
            sites_via_managed: 0,
            total_sites: 0,
            managed_audit: Vec::new(),
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
    match e {
        Expr::AddrOf { operand, .. } => Some(operand),
        Expr::Unary {
            op: hy_syntax::UnOp::Addr,
            operand,
            ..
        } => Some(operand),
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
/// `root_ident()` stops at anything that is not an ident/field/index chain:
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
        Expr::Ident { name, .. } => out.push(name.clone()),
        Expr::Try { operand, .. } => collect_ref_roots(operand, out),
        Expr::Slice { base, .. } => collect_ref_roots(base, out),
        Expr::Field { base, .. } => collect_ref_roots(base, out),
        Expr::Index { base, .. } => collect_ref_roots(base, out),
        Expr::Call { callee, args, .. } => {
            for a in args {
                collect_ref_roots(&a.value, out);
            }
            // `x := r.method(...)` — the receiver is the callee's base, not a
            // call argument, and a method can hand back interior storage.
            if let Expr::Field { base, .. } = unwrap_parens(callee) {
                collect_ref_roots(base, out);
            }
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
        _ => {}
    }
}

/// Collect every `move <operand>` inside `e` (top-level or nested inside
/// composite literals / parens) so nested moves freeze their binding and
/// non-binding GC field moves trigger HYB-E3004 (O5.1 / O5.4).
pub(crate) fn collect_move_operands<'e>(e: &'e Expr, out: &mut Vec<(&'e Expr, Span)>) {
    match e {
        Expr::Move { operand, span } => {
            out.push((unwrap_parens(operand), *span));
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
        _ => {}
    }
}

