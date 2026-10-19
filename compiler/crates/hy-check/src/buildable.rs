//! buildable.rs — issue #134: an accepted program must emit buildable Go.
//!
//! Go's front-end rejects four declaration shapes that `hyb check` used to
//! accept with 0 diagnostics, so the emitted module could never build:
//!
//!   1. an unused `:=` binding — `x := 5` with no read of `x` is
//!      `declared and not used`; `_ := 5` (and `_, _ := ...`) declares no
//!      new variable at all (`no new variables on left side of :=`);
//!   2. an unused import — the emitter carries every declared Go import
//!      into the header, and Go rejects `imported and not used`;
//!   3. a repeated name on the left of one `:=` — `x, x := 1, 2` is
//!      `x repeated on left side of :=`;
//!   4. unused function-local `var` or `const` variables emitted as
//!      Go locals — rejected with `declared and not used`.
//!
//! All four reject fail-closed as HYB-E1002 (registry rule `BUILDABLE`):
//! what the checker accepts must survive `go build`.
//!
//! Issue #425 extends this file-level buildability pass to operand types
//! and binding-shaped builtin calls:
//!   * A. non-boolean `if` / `for` conditions (HYB-E1002)
//!   * B. operator / operand type mismatches (`!x` on non-bool, `-s` on
//!        non-numeric, `x + s` mismatched types, `x && true` on non-bool,
//!        `b < false` ordered compare on non-ordered type, `s++` on
//!        non-numeric) (HYB-E1002)
//!   * C. indexing or slicing a non-indexable / non-sliceable value
//!        (`x[0]`, `x[0:1]`) (HYB-E1002)
//!   * D. calling a resolved non-function value (`x()`) (HYB-E1002)
//!   * E. builtin operand-kind misuse through a binding (`len(x)`,
//!        `close(x)`, `cap(m)`, `delete(xs, 0)`, `append(x, 1)`)
//!        (HYB-E2005, extending #187 from literals to resolved bindings).
//!
//! The read-set is deliberately Go-faithful, not conservative-in-the-wrong-
//! direction: a name is "read" when Go's own unused-variable check counts it
//! — any mention in the function, including inside closure bodies, and
//! including `x++`, `x += 1`, `x[k] = v`, `x.f = v` and `&x` — while a bare
//! `x = v` store is NOT a read (gc reports `declared and not used` for a
//! write-only local). The same-scope re-define (`x, y := 1, 2` after
//! `x := 0`) keeps the existing variable, exactly as Go's `:=` does.
//!
//! Out of scope, documented: select-receive names and match bindings
//! (distinct source binding paths), unread `for ... in` bindings (lowered to
//! `_` at emission, issue #139), and unused `rust/` imports (they are never
//! emitted as Go imports, so they cannot break the build).

use hy_diag::{Diagnostic, Span};
use hy_syntax::{
    AssignOp, BinOp, Block, CallArg, Decl, Expr, FieldInit, File, FnDecl, ForKind, InterpPart,
    LitKind, LocalDecl, LocalKind, MatchPattern, Place, PlaceKind, SelectArmKind, Stmt, TypeExpr,
    UnOp,
};
use std::collections::{HashMap, HashSet};

use crate::model::{as_addr_of, unwrap_parens, Checker};

/// Lightweight operand-kind classification for issue #425: distinguishes
/// `Int`, `Float`, and `Bool` (which `TypeShape::Primitive` collapses) so
/// non-boolean conditions, operator/operand type mismatches, non-indexable
/// index/slice bases, non-function calls, and binding-shaped builtin misuse
/// are rejected at check time rather than failing `go build`.
/// `Unknown` stays permissive (fail-open for operand checks; concurrency
/// boundaries remain fail-closed in `sendability.rs`).
#[derive(Debug, Clone, PartialEq, Eq)]
enum OpKind {
    Unknown,
    /// A known interface type (`any`) — #682 U9-U12.
    ///
    /// Deliberately NOT `Unknown`. `Unknown` means "the checker could not
    /// resolve this" and stays fail-open everywhere. `any` is a *resolved* type
    /// that Go refuses to operate on: you cannot add to it, index it, select a
    /// field from it, or call it. Treating the two identically is what let
    /// those four classes through.
    ///
    /// Assigning *to* an `Interface` target stays legal (an interface accepts
    /// any value), and assigning *from* one stays fail-open because type
    /// assertions are not modelled.
    Interface,
    Int,
    Float,
    Bool,
    Str,
    Slice(Box<OpKind>),
    Array(Box<OpKind>),
    Map(Box<OpKind>, Box<OpKind>),
    Chan(Box<OpKind>),
    Struct(Option<String>),
    Pointer(Box<OpKind>),
    /// Function kind: result kinds, plus the declared parameter count when the
    /// signature is known (`None` = unknown, e.g. a `fn`-typed value whose
    /// declaration is out of reach). The count is what closes the
    /// closure/method-value arity bypass (#577 C9).
    Fn(Vec<OpKind>, Option<usize>),
    Void,
}

impl OpKind {
    fn is_numeric(&self) -> bool {
        matches!(self, OpKind::Int | OpKind::Float)
    }

    fn label(&self) -> String {
        match self {
            OpKind::Unknown => "unknown".to_string(),
            OpKind::Interface => "any".to_string(),
            OpKind::Int => "int".to_string(),
            OpKind::Float => "float64".to_string(),
            OpKind::Bool => "bool".to_string(),
            OpKind::Str => "string".to_string(),
            OpKind::Slice(e) => format!("[]{}", e.label()),
            OpKind::Array(e) => format!("[N]{}", e.label()),
            OpKind::Map(k, v) => format!("map[{}]{}", k.label(), v.label()),
            OpKind::Chan(e) => format!("chan {}", e.label()),
            OpKind::Struct(Some(n)) => format!("struct {}", n),
            OpKind::Struct(None) => "struct".to_string(),
            OpKind::Pointer(inner) => format!("*{}", inner.label()),
            OpKind::Fn(..) => "fn".to_string(),
            OpKind::Void => "void".to_string(),
        }
    }
}

/// Does a value of kind `got` fail to satisfy a declared type of kind `want`,
/// in a way Go rejects at build time (#682)?
///
/// Deliberately conservative, because this is an accept-side check: every
/// "unknown" stays permissive, and the one legal implicit numeric conversion
/// is allowed. `false` means "not provably a build failure", not "provably
/// fine".
///
/// NOTE: open PR #620 introduces this same helper for return-statement
/// checking. Whichever of the two lands second should drop one copy — the two
/// are textually identical on purpose.
fn kinds_conflict(want: &OpKind, got: &OpKind) -> bool {
    use OpKind::*;
    match (want, got) {
        // Fail-open: an unresolved side carries no claim.
        (Unknown, _) | (_, Unknown) => false,
        // An interface target accepts ANY value (`var a any = 1; a = "str"`
        // builds), so `Interface` on the `want` side is never a conflict.
        // `Interface` on the `got` side is fail-open too: assigning an `any` to
        // a concrete type needs a type assertion the kind model does not track,
        // so it must not be rejected on kind evidence alone.
        //
        // Without this arm the U16 assignment check and the U9-U12 interface
        // classification contradict each other: `a` of type `any` resolves to
        // `Interface`, and `Interface` vs `Str` falls through to the mismatch
        // arm below, so every assignment to an `any` target would be rejected.
        // The accept-side corpus controls caught exactly that.
        (Interface, _) | (_, Interface) => false,
        // No value is involved; a count check owns that case.
        (Void, _) | (_, Void) => false,
        // An untyped int constant is assignable to a float target
        // (`var f float64 = 1` builds). The kind model cannot tell an untyped
        // constant from an `int` variable, so this direction must stay allowed
        // or the check would reject buildable programs.
        // No value is involved; the count check owns that case.
        (Void, _) | (_, Void) => false,
        // An untyped int constant is assignable to a float result
        // (`fn f() float64 { return 1 }` builds). The kind model cannot tell
        // an untyped constant from an `int` variable, so this direction must
        // stay allowed or the check would reject buildable programs.
        (Float, Int) => false,
        // A struct whose name did not resolve is not evidence of a mismatch.
        (Struct(None), _) | (_, Struct(None)) => false,
        (a, b) if a == b => false,
        // Same container shape: recurse into the element kinds.
        (Slice(a), Slice(b)) | (Array(a), Array(b)) | (Chan(a), Chan(b)) => {
            kinds_conflict(a, b)
        }
        (Map(k1, v1), Map(k2, v2)) => kinds_conflict(k1, k2) || kinds_conflict(v1, v2),
        (Pointer(a), Pointer(b)) => kinds_conflict(a, b),
        // Fn kinds only carry results here, so a result match is not proof of
        // a signature match; stay permissive.
        (Fn(..), Fn(..)) => false,
        // Everything else is a shape or scalar-family mismatch Go refuses.
        _ => true,
    }
}

/// Symmetrical conflict check for equality operators (`==`, `!=`).
///
/// Untyped numeric constants adapt between integer and floating types
/// in Go comparisons (`1 == 1.0` builds), so numeric families do not
/// conflict. Across different families (e.g. numeric vs string/bool,
/// pointer vs scalar), Go never permits comparison.
fn equality_kinds_conflict(a: &OpKind, b: &OpKind) -> bool {
    if a.is_numeric() && b.is_numeric() {
        return false;
    }
    kinds_conflict(a, b) && kinds_conflict(b, a)
}

/// Is every part of this kind resolved? (#682 U2/U3)
///
/// A "has no field or method" claim is only sound when the checker actually
/// knows the type. `*atomic.int64` resolves to `Pointer(Unknown)` because
/// `sync/atomic`'s generic type is foreign to the kind model, and `hits.add(1)`
/// on it is perfectly good Go — so the field/method check must stay silent
/// whenever an `Unknown` appears anywhere in the shape.
fn is_fully_resolved(k: &OpKind) -> bool {
    // Patterns are written fully qualified on purpose. With `use OpKind::*`, an
    // arm naming a variant that does NOT exist silently becomes an irrefutable
    // binding pattern and matches EVERY kind. Fully-qualified paths turn a
    // missing variant into a compile error instead.
    match k {
        OpKind::Unknown => false,
        // An `any` is resolved as a TYPE, but its members are invisible to the
        // kind model (it may hold anything), so a member lookup on it cannot
        // claim absence — same fail-open as `kinds_conflict`'s Interface arm.
        OpKind::Interface => false,
        OpKind::Int | OpKind::Float | OpKind::Bool | OpKind::Str => true,
        OpKind::Void => false,
        // A struct whose NAME did not resolve cannot have its members looked up.
        OpKind::Struct(None) => false,
        OpKind::Struct(Some(_)) => true,
        OpKind::Slice(e) | OpKind::Array(e) | OpKind::Chan(e) | OpKind::Pointer(e) => {
            is_fully_resolved(e)
        }
        OpKind::Map(k, v) => is_fully_resolved(k) && is_fully_resolved(v),
        // A fn kind only carries results here, so it is not evidence about
        // members either way.
        OpKind::Fn(..) => false,
    }
}

/// Method names that `hy-emit` LOWERS as Hybrid intrinsics instead of emitting
/// a Go method call, so the type they appear on does not need to declare them.
///
/// `stack.pop()` on a `[]int` is example 08's stack shape: `expr.rs` emits
/// `hy_pop(&stack)`, a prelude helper. Go has no method on a slice, so a
/// "no field or method" check that ignored this list would reject buildable
/// programs.
///
/// KEEP IN SYNC with the Field-name special cases in
/// `hy-emit/src/expr.rs`. `pop` is currently the only one; the others there are
/// package-qualified calls (`fmt.print`, `httptest.get`), which are `Ident`
/// callees and never reach this check.
const LOWERED_METHOD_INTRINSICS: &[&str] = &["pop"];

/// One function-local binding that Go requires to be read: `:=`/`let`
/// (checked since #185), `var`/`const` (PR #475), and the elidable
/// `for`/`select` bindings (PR #536 — only diagnosed when written).
/// Tracked for both reads and writes.
struct Define {
    name: String,
    span: Span,
    kind: LocalKind,
    used: bool,
    written: bool,
    elidable: bool,
}

struct Collector<'a> {
    ck: &'a Checker<'a>,
    /// One layer per open block/closure. `Some(id)` = a checked define
    /// (`:=`/`let`/`var`/`const`, or a written elidable `for`/`select`
    /// binding); `None` = a name that shadows (params, receivers, named
    /// results, untouched `for`/select/match bindings) outside the read rule.
    scopes: Vec<HashMap<String, Option<usize>>>,
    /// Parallel lexical scope stack tracking resolved operand kinds (#425).
    kind_scopes: Vec<HashMap<String, OpKind>>,
    /// File-level `const` kinds (#425).
    file_consts: HashMap<String, OpKind>,
    /// Result kinds of the function whose body is currently being walked
    /// (#577 C9/C10). Empty for a fn declared with no results — which is also
    /// the state outside any body, so `Stmt::Return` can only ever see it
    /// while a body is open. `FnLit` bodies swap it like `fn_decl` does.
    fn_results: Vec<OpKind>,
    /// Whether every result of the enclosing signature is named, so Go's
    /// naked `return` is legal (corpus 63).
    fn_results_all_named: bool,
    /// Whether the enclosing signature's last result is `error`, which the
    /// emitter zero-fills — a short return is then lowered to the full arity
    /// (`return v` in a `(value, error)` fn emits `return v, nil`, the E3
    /// sugar; corpus 13).
    fn_results_error_tail: bool,
    /// Lexical nesting depth within loops (`for` / `loop`).
    loop_depth: usize,
    /// Lexical nesting depth within `select` statements.
    select_depth: usize,
    defines: Vec<Define>,
    /// Every identifier mentioned in the file plus every dotted type root —
    /// the import-usage set (the emitter's declared-import header needs it).
    roots: HashSet<String>,
    diags: Vec<Diagnostic>,
    path: String,
}

/// Entry point: `check_file` calls this once per file (after the per-fn
/// passes; diagnostics are sorted into source order by the caller).
pub(crate) fn check_buildable(ck: &mut Checker<'_>) {
    let file: &File = ck.file;
    let diags = {
        let mut c = Collector {
            ck,
            scopes: Vec::new(),
            kind_scopes: Vec::new(),
            file_consts: HashMap::new(),
            fn_results: Vec::new(),
            fn_results_all_named: false,
            fn_results_error_tail: false,
            loop_depth: 0,
            select_depth: 0,
            defines: Vec::new(),
            roots: HashSet::new(),
            diags: Vec::new(),
            path: ck.path.clone(),
        };
        for d in &file.decls {
            if let Decl::Const(cd) = d {
                let k = cd
                    .ty
                    .as_ref()
                    .map(|t| c.kind_of_ty(t, 0))
                    .unwrap_or_else(|| c.kind_of_expr(&cd.value));
                c.file_consts.insert(cd.name.clone(), k);
            }
        }
        for d in &file.decls {
            match d {
                Decl::Import { .. } => {}
                Decl::Const(cd) => {
                    c.ty_opt(&cd.ty);
                    c.expr(&cd.value);
                }
                Decl::Struct(s) => {
                    for f in &s.fields {
                        c.ty(&f.ty);
                    }
                }
                Decl::Enum(e) => {
                    for v in &e.variants {
                        if let Some(t) = &v.payload {
                            c.ty(t);
                        }
                    }
                }
                Decl::TypeAlias(t) => c.ty(&t.ty),
                Decl::Fn(f) => c.fn_decl(f),
            }
        }
        c.check_imports(file);
        c.finish();
        c.diags
    };
    ck.diags.extend(diags);
}

impl<'a> Collector<'a> {
    // -- scope bookkeeping ---------------------------------------------------

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
        self.kind_scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
        self.kind_scopes.pop();
    }

    /// A name that shadows checked defines but is not itself checked.
    fn exempt(&mut self, name: &str) {
        if name != "_" {
            if let Some(top) = self.scopes.last_mut() {
                top.insert(name.to_string(), None);
            }
        }
    }

    fn bind_kind(&mut self, name: &str, kind: OpKind) {
        if name != "_" {
            if let Some(top) = self.kind_scopes.last_mut() {
                top.insert(name.to_string(), kind);
            }
        }
    }

    fn lookup_kind(&self, name: &str) -> Option<OpKind> {
        for layer in self.kind_scopes.iter().rev() {
            if let Some(k) = layer.get(name) {
                return Some(k.clone());
            }
        }
        self.file_consts.get(name).cloned()
    }

    /// A `for`-iterate or `select`-receive binding is elided to `_` at
    /// emission when completely untouched, but if the body assigns to it
    /// (`v = ...`) without ever reading it, the emitter must keep the binding
    /// and Go rejects it as `declared and not used`.
    fn define_elidable(&mut self, name: &str, span: Span) {
        if name == "_" {
            return;
        }
        let Some(top) = self.scopes.last_mut() else {
            return;
        };
        if top.contains_key(name) {
            return;
        }
        let id = self.defines.len();
        self.defines.push(Define {
            name: name.to_string(),
            span,
            kind: LocalKind::Define,
            used: false,
            written: false,
            elidable: true,
        });
        top.insert(name.to_string(), Some(id));
    }

    /// Every identifier mention not shadowed by a local or parameter binding
    /// feeds the import-usage set.
    fn mention(&mut self, name: &str) {
        if !self
            .scopes
            .iter()
            .rev()
            .any(|layer| layer.contains_key(name))
        {
            self.roots.insert(name.to_string());
        }
    }

    /// A read of `name`: marks the innermost checked define it resolves to.
    /// A read inside a closure resolves through the closure's scope layers
    /// to the captured outer define — exactly what Go's check counts.
    fn read(&mut self, name: &str) {
        for layer in self.scopes.iter().rev() {
            if let Some(entry) = layer.get(name) {
                if let Some(id) = entry {
                    self.defines[*id].used = true;
                }
                return;
            }
        }
        self.mention(name);
    }

    fn write(&mut self, name: &str) {
        for layer in self.scopes.iter().rev() {
            if let Some(entry) = layer.get(name) {
                if let Some(id) = entry {
                    self.defines[*id].written = true;
                }
                return;
            }
        }
        self.mention(name);
    }

    fn check_repeated_bindings(&mut self, names: &[String], span: Span) {
        let mut seen: HashSet<&str> = HashSet::new();
        for name in names {
            if name != "_" && !seen.insert(name.as_str()) {
                self.diags.push(
                    Diagnostic::error(
                        "HYB-E1002",
                        self.path.clone(),
                        span,
                        format!(
                            "`{}` is repeated on the left of one `:=` — Go refuses `{} := ...` (repeated on left side of :=) (HYB-E1002)",
                            name,
                            names.join(", ")
                        ),
                    )
                    .with_notes(vec![
                        "each left-hand name must appear once; write `x, y := 1, 2`"
                            .to_string(),
                    ]),
                );
            }
        }
    }

    // -- declarations --------------------------------------------------------

    fn fn_decl(&mut self, f: &FnDecl) {
        if let Some(r) = &f.receiver {
            self.ty(&r.ty);
        }
        for p in &f.params {
            self.ty(&p.ty);
        }
        for r in &f.results {
            self.ty(&r.ty);
        }
        let result_kinds: Vec<OpKind> = f
            .results
            .iter()
            .map(|r| self.kind_of_ty(&r.ty, 0))
            .collect();
        let all_named = !f.results.is_empty() && f.results.iter().all(|r| r.name.is_some());
        let error_tail = matches!(
            f.results.last().map(|r| &r.ty),
            Some(TypeExpr::Named { name, .. }) if name == "error"
        );
        let Some(body) = &f.body else { return };
        self.push_scope();
        if let Some(r) = &f.receiver {
            self.exempt(&r.name);
            let rk = self.kind_of_ty(&r.ty, 0);
            let rk = if r.is_pointer {
                OpKind::Pointer(Box::new(rk))
            } else {
                rk
            };
            self.bind_kind(&r.name, rk);
        }
        for p in &f.params {
            self.exempt(&p.name);
            let pk = self.kind_of_ty(&p.ty, 0);
            self.bind_kind(&p.name, pk);
        }
        for r in &f.results {
            if let Some(rname) = &r.name {
                self.exempt(rname);
                let rk = self.kind_of_ty(&r.ty, 0);
                self.bind_kind(rname, rk);
            }
        }
        for r in &f.results {
            if let Some(name) = &r.name {
                self.exempt(name);
            }
        }
        let outer = std::mem::replace(&mut self.fn_results, result_kinds);
        let outer_named = std::mem::replace(&mut self.fn_results_all_named, all_named);
        let outer_err = std::mem::replace(&mut self.fn_results_error_tail, error_tail);
        let outer_loop = std::mem::replace(&mut self.loop_depth, 0);
        let outer_select = std::mem::replace(&mut self.select_depth, 0);
        self.block_stmts(body);
        self.loop_depth = outer_loop;
        self.select_depth = outer_select;
        self.fn_results = outer;
        self.fn_results_all_named = outer_named;
        self.fn_results_error_tail = outer_err;
        self.pop_scope();
    }

    fn local(&mut self, l: &LocalDecl) {
        // Only the short declaration operator needs freshness checks.
        // Every kind of fn-local declaration emits a Go binding, whose
        // name must be read somewhere in its enclosing function.
        let short_define = matches!(l.kind, LocalKind::Define | LocalKind::Let);
        let track_read = matches!(
            l.kind,
            LocalKind::Define | LocalKind::Let | LocalKind::Var | LocalKind::Const
        );
        if short_define {
            // Form 3: `x, x := 1, 2` — `x repeated on left side of :=`.
            self.check_repeated_bindings(&l.names, l.span);
            // Form 1 (the `_` half): `:=` needs at least one NEW variable.
            // `_ := 5`, `_, _ := ...` and a same-scope re-define declare
            // none — Go's `no new variables on left side of :=`.
            let fresh = l.names.iter().any(|n| {
                n != "_"
                    && !self
                        .scopes
                        .last()
                        .is_some_and(|top| top.contains_key(n.as_str()))
            });
            if !fresh {
                self.diags.push(
                    Diagnostic::error(
                        "HYB-E1002",
                        self.path.clone(),
                        l.span,
                        format!(
                            "`{} := ...` declares no new variable — Go refuses it (no new variables on left side of :=) (HYB-E1002)",
                            l.names.join(", ")
                        ),
                    )
                    .with_notes(vec![
                        "`_` is not a variable: `_ := 5` can never build — write `_ = 5` to discard the value, or bind a real name".to_string(),
                        "a name already declared in this same scope is not new either".to_string(),
                    ]),
                );
            }
        }
        if let Some(t) = &l.ty {
            self.ty(t);
        }
        // RHS first: `x := x` reads the OUTER `x` (Go's scoping).
        for v in &l.values {
            self.expr(v);
        }
        // #682 U8: `a := v()` where `v` produces no value. Only the
        // single-name/single-value shape is checked; `a, b := f()` is a legal
        // multi-value binding and is handled by `infer_local_kinds`.
        if l.names.len() == 1 && l.values.len() == 1 {
            self.check_single_value_position(&l.values[0], "a binding", /*binding_position=*/ true);
        }
        // Composed resolution (pr-430 x landed pr-475): #475's track_read
        // set (Define|Let|Var|Const) drives which locals get unused-read
        // tracking; #430's operand-kind inference runs for EVERY name so
        // `kind_of_expr` resolves binding-shaped operands (issue #425).
        let kinds = self.infer_local_kinds(l);
        for (name, kind) in l.names.iter().zip(kinds) {
            self.bind_kind(name, kind);
            if !track_read {
                self.exempt(name);
                continue;
            }
            if name == "_" {
                continue;
            }
            let existing_let = self
                .scopes
                .last()
                .and_then(|top| top.get(name.as_str()).copied().flatten())
                .is_some_and(|id| self.defines[id].kind == LocalKind::Let);
            if existing_let {
                self.emit_let_reassign_error(name, l.span);
                continue;
            }
            let Some(top) = self.scopes.last_mut() else {
                continue;
            };
            if top.contains_key(name.as_str()) {
                // Same-scope re-define: Go keeps the existing variable.
                continue;
            }
            let id = self.defines.len();
            self.defines.push(Define {
                name: name.clone(),
                span: l.span,
                kind: l.kind,
                used: false,
                written: false,
                elidable: false,
            });
            top.insert(name.clone(), Some(id));
        }
    }

    fn infer_local_kinds(&self, l: &LocalDecl) -> Vec<OpKind> {
        let n = l.names.len();
        if let Some(ty) = &l.ty {
            return vec![self.kind_of_ty(ty, 0); n];
        }
        if n == 2 && l.values.len() == 1 {
            return self.multi_value_kinds(&l.values[0]);
        }
        if n == l.values.len() {
            return l.values.iter().map(|v| self.kind_of_expr(v)).collect();
        }
        vec![OpKind::Unknown; n]
    }

    fn multi_value_kinds(&self, e: &Expr) -> Vec<OpKind> {
        match unwrap_parens(e) {
            Expr::Move { operand, .. } => self.multi_value_kinds(operand),
            Expr::Index { base, .. } => match self.kind_of_expr(base) {
                OpKind::Map(_, v) => vec![*v, OpKind::Bool],
                _ => vec![OpKind::Unknown, OpKind::Bool],
            },
            Expr::Receive { chan, .. } => match self.kind_of_expr(chan) {
                OpKind::Chan(elem) => vec![*elem, OpKind::Bool],
                _ => vec![OpKind::Unknown, OpKind::Bool],
            },
            Expr::Call { callee, args, .. } => {
                if let Some(res) = self.call_result_kinds(callee, args) {
                    if res.len() >= 2 {
                        return vec![res[0].clone(), res[1].clone()];
                    }
                }
                vec![OpKind::Unknown, OpKind::Unknown]
            }
            _ => vec![OpKind::Unknown, OpKind::Unknown],
        }
    }

    // -- statements ----------------------------------------------------------

    fn block_stmts(&mut self, b: &Block) {
        for s in &b.stmts {
            self.stmt(s);
        }
    }

    fn block_scoped(&mut self, b: &Block) {
        self.push_scope();
        self.block_stmts(b);
        self.pop_scope();
    }

    fn stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Local(l) => self.local(l),
            Stmt::Assign {
                targets,
                op,
                values,
                span,
            } => {
                for t in targets {
                    self.place(t, *op);
                }
                for v in values {
                    self.expr(v);
                }
                // #682 U8: `a = v()` where `v` produces no value. The
                // multi-name form (`a, b = f()`) is a legal multi-value
                // assignment, so only the single-target/single-value shape is
                // checked here.
                if targets.len() == 1 && values.len() == 1 {
                    self.check_single_value_position(&values[0], "an assignment", /*binding_position=*/ true);
                }
                self.check_assign_operands(targets, *op, values, *span);
            }
            Stmt::Expr { expr, .. } => self.expr(expr),
            Stmt::Send { chan, value, .. } => {
                self.expr(chan);
                self.expr(value);
            }
            Stmt::If {
                init,
                cond,
                then,
                otherwise,
                ..
            } => {
                // Go's if-init scope covers the whole construct.
                self.push_scope();
                if let Some(i) = init {
                    self.stmt(i);
                }
                self.expr(cond);
                self.check_bool_condition(cond, "if");
                self.block_scoped(then);
                if let Some(o) = otherwise {
                    self.stmt(o);
                }
                self.pop_scope();
            }
            Stmt::For {
                kind, body, span, ..
            } => {
                self.push_scope();
                self.loop_depth += 1;
                match kind {
                    ForKind::While { cond } => {
                        self.expr(cond);
                        self.check_bool_condition(cond, "for");
                    }
                    ForKind::Range { var, lo, hi, .. } => {
                        self.expr(lo);
                        self.expr(hi);
                        // The loop head always reads the counter.
                        self.exempt(var);
                        self.bind_kind(var, OpKind::Int);
                    }
                    ForKind::Iterate { vars, expr } => {
                        self.check_repeated_bindings(vars, *span);
                        self.expr(expr);
                        let iter_kinds = self.iterate_var_kinds(expr, vars.len());
                        // Unread iterate bindings are lowered to `_` at
                        // emission (issue #139); write-only ones still
                        // reject as unused (PR #536).
                        for (v, k) in vars.iter().zip(iter_kinds) {
                            self.define_elidable(v, *span);
                            self.bind_kind(v, k);
                        }
                    }
                    ForKind::Infinite => {}
                }
                self.block_scoped(body);
                self.loop_depth -= 1;
                self.pop_scope();
            }
            Stmt::Return { values, span } => {
                for v in values {
                    self.expr(v);
                }
                self.check_return(values, *span);
            }
            Stmt::Task { callee, args, span } => {
                self.expr(callee);
                for a in args {
                    self.expr(&a.value);
                }
                match unwrap_parens(callee) {
                    Expr::Ident { name, .. } => {
                        self.check_user_fn_arity(name, args, *span);
                        self.check_user_fn_arg_types(name, args);
                    }
                    Expr::Field { base, name, .. } => {
                        let bk = self.kind_of_expr(base);
                        self.check_method_arity(&bk, name, args, *span);
                        self.check_method_arg_types(&bk, name, args);
                    }
                    _ => {}
                }
            }
            Stmt::Managed { body, .. } => self.block_scoped(body),
            Stmt::Match { subject, arms, .. } => {
                self.expr(subject);
                for arm in arms {
                    self.push_scope();
                    if let MatchPattern::Path { segments, binding } = &arm.pattern {
                        if let Some(root) = segments.first() {
                            self.mention(root);
                        }
                        if let Some(b) = binding {
                            self.define_elidable(b, subject.span());
                            self.bind_kind(b, OpKind::Unknown);
                        }
                    }
                    if let Some(g) = &arm.guard {
                        self.expr(g);
                        self.check_bool_condition(g, "match guard");
                    }
                    self.block_scoped(&arm.body);
                    self.pop_scope();
                }
            }
            Stmt::Defer { call, span } => {
                self.expr(call);
                // #682 U4: the deferred call's arity is checked like any other
                // call's. `builtins.rs` owns the "must be a call" half.
                self.check_defer_arity(call, *span);
            }
            Stmt::Break { span, label } => {
                if label.is_none() && self.loop_depth == 0 && self.select_depth == 0 {
                    self.diags.push(
                        Diagnostic::error(
                            "HYB-E1002",
                            self.path.clone(),
                            *span,
                            "break is not in a loop or select statement (HYB-E1002)",
                        )
                        .with_notes(vec![
                            "plain `break` can only be used inside a `for` loop or `select` statement".to_string(),
                            "move this statement inside a loop or select, or remove it".to_string(),
                        ]),
                    );
                }
            }
            Stmt::Continue { span, label } => {
                if label.is_none() && self.loop_depth == 0 {
                    self.diags.push(
                        Diagnostic::error(
                            "HYB-E1002",
                            self.path.clone(),
                            *span,
                            "continue is not in a loop (HYB-E1002)",
                        )
                        .with_notes(vec![
                            "`continue` can only be used inside a `for` loop".to_string(),
                            "move this statement inside a loop, or remove it".to_string(),
                        ]),
                    );
                }
            }
            Stmt::Block { block, .. } => self.block_scoped(block),
            Stmt::Select { arms, .. } => {
                self.select_depth += 1;
                for arm in arms {
                    self.push_scope();
                    match &arm.kind {
                        SelectArmKind::Send { chan, value, .. } => {
                            self.expr(chan);
                            self.expr(value);
                        }
                        SelectArmKind::Recv { names, chan } => {
                            self.check_repeated_bindings(names, chan.span());
                            self.expr(chan);
                            let elem = match self.kind_of_expr(chan) {
                                OpKind::Chan(e) => *e,
                                _ => OpKind::Unknown,
                            };
                            for (i, n) in names.iter().enumerate() {
                                self.define_elidable(n, chan.span());
                                self.bind_kind(n, if i == 0 { elem.clone() } else { OpKind::Bool });
                            }
                        }
                        SelectArmKind::Default => {}
                    }
                    self.block_scoped(&arm.body);
                    self.pop_scope();
                }
                self.select_depth -= 1;
            }
        }
    }

    fn iterate_var_kinds(&self, expr: &Expr, n_vars: usize) -> Vec<OpKind> {
        match self.kind_of_expr(expr) {
            OpKind::Slice(e) | OpKind::Array(e) | OpKind::Chan(e) => {
                if n_vars >= 2 {
                    vec![OpKind::Int, *e]
                } else {
                    vec![*e]
                }
            }
            OpKind::Map(k, v) => {
                if n_vars >= 2 {
                    vec![*k, *v]
                } else {
                    vec![*v]
                }
            }
            OpKind::Str => vec![OpKind::Int; n_vars.max(1)],
            _ => vec![OpKind::Unknown; n_vars.max(1)],
        }
    }

    // -- places (assignment targets) ----------------------------------------

    fn place(&mut self, p: &Place, op: AssignOp) {
        match &p.kind {
            PlaceKind::Ident(n) => {
                // A bare `x = v` store is the ONE target form Go does not
                // count as a read; `x++`, `x += 1` and every non-ident
                // target (`x[k] = v`, `x.f = v`, `&x`) read the root.
                if matches!(op, AssignOp::Assign) {
                    self.write(n);
                } else {
                    self.read(n);
                }
            }
            PlaceKind::Index { .. } | PlaceKind::Field { .. } | PlaceKind::AddrOf { .. } => {
                self.place_read(p)
            }
        }
    }

    fn emit_let_reassign_error(&mut self, name: &str, span: Span) {
        self.diags.push(
            Diagnostic::error(
                "HYB-E3006",
                self.path.clone(),
                span,
                format!(
                    "cannot assign to `{}` — it is a `let` binding (immutable binding, T22) (HYB-E3006)",
                    name
                ),
            )
            .with_notes(vec![
                "`let` declares an immutable binding: reassigning `x = ...`, `x += ...`, or `x++` is rejected (SPEC-LANGUAGE T22 / D14)".to_string(),
                "declare with `var x = ...` or `x := ...` if the binding must be reassigned".to_string(),
            ]),
        );
    }

    fn place_read(&mut self, p: &Place) {
        match &p.kind {
            PlaceKind::Ident(n) => self.read(n),
            PlaceKind::Index { base, index } => {
                self.place_read(base);
                self.expr(index);
                let bk = self.kind_of_place(base);
                let ik = self.kind_of_expr(index);
                self.check_index_kind(&base.render(), None, &bk, index, &ik, p.span);
            }
            PlaceKind::Field { base, .. } => self.place_read(base),
            PlaceKind::AddrOf { base } => self.place_read(base),
        }
    }

    // -- expressions and types ----------------------------------------------

    fn expr(&mut self, e: &Expr) {
        match e {
            Expr::Ident { name, .. } => self.read(name),
            Expr::Lit { .. } => {}
            Expr::Interp { parts, .. } => {
                for p in parts {
                    if let InterpPart::Expr(x) = p {
                        self.expr(x);
                    }
                }
            }
            Expr::Binary {
                op, lhs, rhs, span, ..
            } => {
                self.expr(lhs);
                self.expr(rhs);
                self.check_binary_operands(*op, lhs, rhs, *span);
            }
            Expr::Unary {
                op, operand, span, ..
            } => {
                self.expr(operand);
                self.check_unary_operand(*op, operand, *span);
            }
            Expr::AddrOf { operand, .. } => self.expr(operand),
            Expr::Move { operand, .. } => self.expr(operand),
            Expr::Try { operand, .. } => self.expr(operand),
            Expr::Receive { chan, .. } => self.expr(chan),
            Expr::Index {
                base, index, span, ..
            } => {
                self.expr(base);
                self.expr(index);
                let bk = self.kind_of_expr(base);
                let ik = self.kind_of_expr(index);
                self.check_index_kind(&base.render(), Some(base.as_ref()), &bk, index, &ik, *span);
            }
            Expr::Slice {
                base, lo, hi, span, ..
            } => {
                self.expr(base);
                if let Some(x) = lo {
                    self.expr(x);
                }
                if let Some(x) = hi {
                    self.expr(x);
                }
                let bk = self.kind_of_expr(base);
                self.check_slice_kind(base, &bk, lo.as_deref(), hi.as_deref(), *span);
            }
            // #682 U10: Go refuses `a.nope` where `a` is `any`
            // (`a.nope undefined (type any has no field or method nope)`).
            // The struct-field lookup below cannot catch it, because an
            // interface is not a struct — so reject the base explicitly.
            Expr::Field { base, name, span } => {
                self.check_pointer_method_receiver(base, name, *span);
                if matches!(self.kind_of_expr(base), OpKind::Interface) {
                    self.emit_e1002(
                        *span,
                        format!(
                            "cannot select field `{}` on a value of type `any` — Go refuses `{}.{} undefined (type any has no field or method {})` (HYB-E1002)",
                            name,
                            base.render(),
                            name,
                            name
                        ),
                        "an `any` value must be asserted to a concrete type before its fields can be read",
                    );
                }
                self.expr(base);
            }
            Expr::Call {
                callee, args, span, ..
            } => {
                self.expr(callee);
                for a in args {
                    self.expr(&a.value);
                }
                // #682 U7: every argument must be exactly one value, except
                // that a lone multi-value call IS the whole argument list when
                // the callee takes that many parameters (`g(two())` for a
                // 2-param `g` builds). That exception is checked against the
                // resolved arity in `check_user_fn_arity`; here the shape is
                // only rejected when the callee cannot take it.
                if self.multi_value_arg_count(args).is_none() {
                    for a in args {
                        self.check_single_value_position(&a.value, "a call argument", /*binding_position=*/ false);
                    }
                } else {
                    self.check_multi_value_arg_arity(callee, args, *span);
                }
                self.check_call_operands(callee, args, *span);
            }
            Expr::StructLit { ty, fields, span } => {
                if let Some(t) = ty {
                    self.ty(t);
                }
                for f in fields {
                    self.expr(&f.value);
                }
                self.check_struct_literal(ty.as_ref(), fields, *span);
            }
            Expr::SliceLit { elem, items, .. } => {
                if let Some(t) = elem {
                    self.ty(t);
                }
                for i in items {
                    self.expr(i);
                }
                if let Some((_, inner_elem, _)) = e.as_array_lit() {
                    self.check_array_lit_items(inner_elem, items);
                }
            }
            Expr::MapLit {
                key,
                value,
                entries,
                ..
            } => {
                self.ty(key);
                self.ty(value);
                for (k, v) in entries {
                    self.expr(k);
                    self.expr(v);
                }
            }
            Expr::FnLit {
                params,
                results,
                body,
                ..
            } => {
                for p in params {
                    self.ty(&p.ty);
                }
                for r in results {
                    self.ty(&r.ty);
                }
                self.push_scope();
                for p in params {
                    self.exempt(&p.name);
                    let pk = self.kind_of_ty(&p.ty, 0);
                    self.bind_kind(&p.name, pk);
                }
                for r in results {
                    if let Some(rname) = &r.name {
                        self.exempt(rname);
                        let rk = self.kind_of_ty(&r.ty, 0);
                        self.bind_kind(rname, rk);
                    }
                }
                for r in results {
                    if let Some(name) = &r.name {
                        self.exempt(name);
                    }
                }
                let result_kinds: Vec<OpKind> = results
                    .iter()
                    .map(|r| self.kind_of_ty(&r.ty, 0))
                    .collect();
                let all_named =
                    !results.is_empty() && results.iter().all(|r| r.name.is_some());
                let error_tail = matches!(
                    results.last().map(|r| &r.ty),
                    Some(TypeExpr::Named { name, .. }) if name == "error"
                );
                let outer = std::mem::replace(&mut self.fn_results, result_kinds);
                let outer_named = std::mem::replace(&mut self.fn_results_all_named, all_named);
                let outer_err = std::mem::replace(&mut self.fn_results_error_tail, error_tail);
                let outer_loop = std::mem::replace(&mut self.loop_depth, 0);
                let outer_select = std::mem::replace(&mut self.select_depth, 0);
                self.block_stmts(body);
                self.loop_depth = outer_loop;
                self.select_depth = outer_select;
                self.fn_results = outer;
                self.fn_results_all_named = outer_named;
                self.fn_results_error_tail = outer_err;
                self.pop_scope();
            }
            Expr::Paren { inner, .. } => self.expr(inner),
        }
    }

    fn ty_opt(&mut self, t: &Option<TypeExpr>) {
        if let Some(t) = t {
            self.ty(t);
        }
    }

    fn ty(&mut self, t: &TypeExpr) {
        match t {
            TypeExpr::Named { name, span } => {
                self.record_type_name_roots(name);
                self.check_named_type_instantiation(name, *span);
            }
            TypeExpr::Pointer { to, .. } => self.ty(to),
            TypeExpr::Slice { elem, .. } => self.ty(elem),
            TypeExpr::Array { len, elem, .. } => {
                if let Some(l) = len {
                    self.expr(l);
                }
                self.ty(elem);
            }
            TypeExpr::Map { key, value, .. } => {
                self.ty(key);
                self.ty(value);
            }
            TypeExpr::Chan { elem, .. } => self.ty(elem),
            TypeExpr::Option { elem, .. } => self.ty(elem),
            TypeExpr::Fn {
                params, results, ..
            } => {
                for p in params {
                    self.ty(&p.ty);
                }
                for r in results {
                    self.ty(r);
                }
            }
        }
    }

    fn record_type_name_roots(&mut self, name: &str) {
        for seg in name.split(|c: char| matches!(c, '[' | ']' | ',' | '*' | ' ')) {
            let seg = seg.trim();
            if let Some((root, _)) = seg.split_once('.') {
                if !root.is_empty() {
                    self.roots.insert(root.to_string());
                }
            }
        }
    }

    fn check_named_type_instantiation(&mut self, name: &str, span: Span) {
        let name = name.trim();
        if let Some(rest) = name
            .strip_prefix('*')
            .or_else(|| name.strip_prefix("[]"))
            .or_else(|| name.strip_prefix("[...]"))
            .or_else(|| name.strip_prefix("<-chan "))
            .or_else(|| name.strip_prefix("chan<- "))
            .or_else(|| name.strip_prefix("chan "))
        {
            self.check_named_type_instantiation(rest, span);
            return;
        }
        let (head, inner_opt) = match name.find('[') {
            Some(i) if name.ends_with(']') => (&name[..i], Some(&name[i + 1..name.len() - 1])),
            _ => (name, None),
        };
        if let Some(inner) = inner_opt {
            for arg in split_top_level_type_args(inner) {
                self.check_named_type_instantiation(arg, span);
            }
        }
        if head == "set" || head == "option" || head == "map" {
            return;
        }
        let Some(tparams) = self.ck.struct_type_params.get(head) else {
            return;
        };
        let expected = tparams.len();
        match (expected, inner_opt) {
            (0, Some(_)) => {
                self.emit_e1002(
                    span,
                    format!(
                        "`{}` is not a generic type — cannot instantiate `{}` with type arguments (HYB-E1002)",
                        head, name
                    ),
                    "remove the `[...]` type arguments or declare type parameters on the struct",
                );
            }
            (n, None) if n > 0 => {
                self.emit_e1002(
                    span,
                    format!(
                        "cannot use generic struct `{}` without type arguments (expected {}) — Go refuses `cannot use generic type {} without instantiation` (HYB-E1002)",
                        head, n, head
                    ),
                    "instantiate the generic struct with explicit type arguments (e.g. `box[int]`)",
                );
            }
            (n, Some(inner)) if n > 0 => {
                let got = split_top_level_type_args(inner).len();
                if got != n {
                    self.emit_e1002(
                        span,
                        format!(
                            "wrong number of type arguments for `{}` in `{}`: expected {}, found {} (HYB-E1002)",
                            head, name, n, got
                        ),
                        "supply one type argument per declared type parameter on the struct",
                    );
                }
            }
            _ => {}
        }
    }

    // -- issue #425: operand kind resolution & checks -----------------------

    fn kind_of_ty(&self, ty: &TypeExpr, depth: usize) -> OpKind {
        if depth > 8 {
            return OpKind::Unknown;
        }
        match ty {
            TypeExpr::Named { name, .. } => self.kind_of_named(name, depth),
            TypeExpr::Pointer { to, .. } => {
                OpKind::Pointer(Box::new(self.kind_of_ty(to, depth + 1)))
            }
            TypeExpr::Slice { elem, .. } => {
                OpKind::Slice(Box::new(self.kind_of_ty(elem, depth + 1)))
            }
            TypeExpr::Array { elem, .. } => {
                OpKind::Array(Box::new(self.kind_of_ty(elem, depth + 1)))
            }
            TypeExpr::Map { key, value, .. } => OpKind::Map(
                Box::new(self.kind_of_ty(key, depth + 1)),
                Box::new(self.kind_of_ty(value, depth + 1)),
            ),
            TypeExpr::Chan { elem, .. } => {
                OpKind::Chan(Box::new(self.kind_of_ty(elem, depth + 1)))
            }
            TypeExpr::Option { .. } => OpKind::Unknown,
            TypeExpr::Fn { params, results, .. } => OpKind::Fn(
                results
                    .iter()
                    .map(|r| self.kind_of_ty(r, depth + 1))
                    .collect(),
                Some(params.len()),
            ),
        }
    }

    fn kind_of_named(&self, name: &str, depth: usize) -> OpKind {
        if depth > 8 {
            return OpKind::Unknown;
        }
        let name = name.trim();
        let base = Checker::base_named_type_str(name);
        if self.ck.structs.contains_key(base) || self.ck.struct_type_params.contains_key(base) {
            return OpKind::Struct(Some(name.to_string()));
        }
        if let Some(target) = self.ck.type_aliases.get(base) {
            return self.kind_of_ty(target, depth + 1);
        }
        match name {
            // #682 U9-U12: `any` is a resolved interface type, not an
            // unresolved one, so it must not fall through to `Unknown`.
            "any" | "interface{}" => OpKind::Interface,
            "bool" => OpKind::Bool,
            "string" => OpKind::Str,
            "float32" | "float64" | "f32" | "f64" => OpKind::Float,
            "int" | "i8" | "int8" | "i16" | "int16" | "i32" | "int32" | "i64" | "int64" | "u8"
            | "uint8" | "u16" | "uint16" | "u32" | "uint32" | "u64" | "uint64" | "usize"
            | "isize" | "uintptr" | "byte" | "rune" | "char" => OpKind::Int,
            _ => OpKind::Unknown,
        }
    }

    fn kind_of_type_str(&self, s: &str, depth: usize) -> OpKind {
        if depth > 8 {
            return OpKind::Unknown;
        }
        let s = s.trim();
        if let Some(rest) = s.strip_prefix('*') {
            return OpKind::Pointer(Box::new(self.kind_of_type_str(rest, depth + 1)));
        }
        if let Some(rest) = s.strip_prefix("[]") {
            return OpKind::Slice(Box::new(self.kind_of_type_str(rest, depth + 1)));
        }
        if let Some(rest) = s.strip_prefix("[...]") {
            return OpKind::Array(Box::new(self.kind_of_type_str(rest, depth + 1)));
        }
        if let Some(rest) = s
            .strip_prefix("<-chan ")
            .or_else(|| s.strip_prefix("chan<- "))
            .or_else(|| s.strip_prefix("chan "))
        {
            return OpKind::Chan(Box::new(self.kind_of_type_str(rest, depth + 1)));
        }
        self.kind_of_named(s, depth)
    }

    fn kind_of_ty_with_subst(
        &self,
        ty: &TypeExpr,
        subst: &HashMap<String, String>,
        depth: usize,
    ) -> OpKind {
        if depth > 8 {
            return OpKind::Unknown;
        }
        if subst.is_empty() {
            return self.kind_of_ty(ty, depth);
        }
        match ty {
            TypeExpr::Named { name, .. } => {
                if let Some(arg) = subst.get(name.as_str()) {
                    return self.kind_of_type_str(arg, depth + 1);
                }
                self.kind_of_named(name, depth)
            }
            TypeExpr::Pointer { to, .. } => {
                OpKind::Pointer(Box::new(self.kind_of_ty_with_subst(to, subst, depth + 1)))
            }
            TypeExpr::Slice { elem, .. } => {
                OpKind::Slice(Box::new(self.kind_of_ty_with_subst(elem, subst, depth + 1)))
            }
            TypeExpr::Array { elem, .. } => {
                OpKind::Array(Box::new(self.kind_of_ty_with_subst(elem, subst, depth + 1)))
            }
            TypeExpr::Map { key, value, .. } => OpKind::Map(
                Box::new(self.kind_of_ty_with_subst(key, subst, depth + 1)),
                Box::new(self.kind_of_ty_with_subst(value, subst, depth + 1)),
            ),
            TypeExpr::Chan { elem, .. } => {
                OpKind::Chan(Box::new(self.kind_of_ty_with_subst(elem, subst, depth + 1)))
            }
            TypeExpr::Option { .. } => OpKind::Unknown,
            TypeExpr::Fn { results, .. } => OpKind::Fn(
                results
                    .iter()
                    .map(|r| self.kind_of_ty_with_subst(r, subst, depth + 1))
                    .collect(),
                // A bare fn TYPE does not carry a call signature the kind model
                // can trust a count from — keep the arity unknown.
                None,
            ),
        }
    }

    fn struct_type_arg_map(&self, full_name: &str) -> HashMap<String, String> {
        let mut map = HashMap::new();
        let Some(open) = full_name.find('[') else {
            return map;
        };
        if !full_name.ends_with(']') {
            return map;
        }
        let base = &full_name[..open];
        let inner = &full_name[open + 1..full_name.len() - 1];
        let Some(tparams) = self.ck.struct_type_params.get(base) else {
            return map;
        };
        let args = split_top_level_type_args(inner);
        if args.len() != tparams.len() {
            return map;
        }
        for (tp, arg) in tparams.iter().zip(args.into_iter()) {
            map.insert(tp.name.clone(), arg.to_string());
        }
        map
    }

    fn kind_of_place(&self, p: &Place) -> OpKind {
        match &p.kind {
            PlaceKind::Ident(n) => self.lookup_kind(n).unwrap_or(OpKind::Unknown),
            PlaceKind::Index { base, .. } => match self.kind_of_place(base) {
                OpKind::Slice(e) | OpKind::Array(e) => *e,
                OpKind::Map(_, v) => *v,
                OpKind::Str => OpKind::Int,
                OpKind::Pointer(inner) => match *inner {
                    OpKind::Array(e) => *e,
                    _ => OpKind::Unknown,
                },
                _ => OpKind::Unknown,
            },
            PlaceKind::Field { base, name } => {
                let bk = self.kind_of_place(base);
                self.struct_field_kind(&bk, name).unwrap_or(OpKind::Unknown)
            }
            PlaceKind::AddrOf { base } => OpKind::Pointer(Box::new(self.kind_of_place(base))),
        }
    }

    fn struct_name(&self, k: &OpKind) -> Option<String> {
        match k {
            OpKind::Struct(Some(s)) => Some(s.clone()),
            OpKind::Pointer(inner) => self.struct_name(inner),
            _ => None,
        }
    }

    fn struct_base_name(&self, k: &OpKind) -> Option<String> {
        self.struct_name(k)
            .map(|s| Checker::base_named_type_str(&s).to_string())
    }

    fn struct_field_kind(&self, k: &OpKind, field: &str) -> Option<OpKind> {
        let full_name = self.struct_name(k)?;
        let base_name = Checker::base_named_type_str(&full_name);
        let fields = self.ck.structs.get(base_name)?;
        let subst = self.struct_type_arg_map(&full_name);
        fields
            .iter()
            .find(|(n, _)| n == field)
            .map(|(_, ty)| self.kind_of_ty_with_subst(ty, &subst, 0))
    }

    fn kind_of_expr(&self, e: &Expr) -> OpKind {
        match e {
            Expr::Ident { name, .. } => {
                if let Some(k) = self.lookup_kind(name) {
                    return k;
                }
                if name == "true" || name == "false" {
                    return OpKind::Bool;
                }
                let head = Checker::base_named_type_str(name);
                if let Some(f) = self.ck.fns.get(name.as_str()).or_else(|| self.ck.fns.get(head)) {
                    return OpKind::Fn(
                        f.results
                            .iter()
                            .map(|r| self.kind_of_ty(&r.ty, 0))
                            .collect(),
                        Some(f.params.len()),
                    );
                }
                OpKind::Unknown
            }
            Expr::Lit { kind, .. } => match kind {
                LitKind::Int(_) | LitKind::Char(_) => OpKind::Int,
                LitKind::Float(_) => OpKind::Float,
                LitKind::Bool(_) => OpKind::Bool,
                LitKind::Str(_) | LitKind::RawStr(_) => OpKind::Str,
                LitKind::Nil => OpKind::Unknown,
            },
            Expr::Interp { .. } => OpKind::Str,
            Expr::Paren { inner, .. }
            | Expr::Move { operand: inner, .. }
            | Expr::Try { operand: inner, .. } => self.kind_of_expr(inner),
            Expr::AddrOf { operand, .. } => OpKind::Pointer(Box::new(self.kind_of_expr(operand))),
            Expr::Unary { op, operand, .. } => match op {
                UnOp::Addr => OpKind::Pointer(Box::new(self.kind_of_expr(operand))),
                UnOp::Not => OpKind::Bool,
                UnOp::Neg => match self.kind_of_expr(operand) {
                    OpKind::Float => OpKind::Float,
                    OpKind::Int => OpKind::Int,
                    _ => OpKind::Unknown,
                },
            },
            Expr::Binary { op, lhs, rhs, .. } => match op {
                BinOp::Eq
                | BinOp::Ne
                | BinOp::Lt
                | BinOp::Le
                | BinOp::Gt
                | BinOp::Ge
                | BinOp::And
                | BinOp::Or => OpKind::Bool,
                BinOp::Add => {
                    let lk = self.kind_of_expr(lhs);
                    let rk = self.kind_of_expr(rhs);
                    if lk == OpKind::Str && rk == OpKind::Str {
                        OpKind::Str
                    } else if lk.is_numeric() && rk.is_numeric() {
                        if lk == OpKind::Float || rk == OpKind::Float {
                            OpKind::Float
                        } else {
                            OpKind::Int
                        }
                    } else {
                        OpKind::Unknown
                    }
                }
                BinOp::Sub | BinOp::Mul | BinOp::Div => {
                    let lk = self.kind_of_expr(lhs);
                    let rk = self.kind_of_expr(rhs);
                    if lk.is_numeric() && rk.is_numeric() {
                        if lk == OpKind::Float || rk == OpKind::Float {
                            OpKind::Float
                        } else {
                            OpKind::Int
                        }
                    } else {
                        OpKind::Unknown
                    }
                }
                BinOp::Mod
                | BinOp::BitOr
                | BinOp::BitXor
                | BinOp::BitAnd
                | BinOp::Shl
                | BinOp::Shr => {
                    let lk = self.kind_of_expr(lhs);
                    let rk = self.kind_of_expr(rhs);
                    if lk == OpKind::Int && rk == OpKind::Int {
                        OpKind::Int
                    } else {
                        OpKind::Unknown
                    }
                }
            },
            Expr::Receive { chan, .. } => match self.kind_of_expr(chan) {
                OpKind::Chan(elem) => *elem,
                _ => OpKind::Unknown,
            },
            Expr::Index { base, .. } => match self.kind_of_expr(base) {
                OpKind::Slice(e) | OpKind::Array(e) => *e,
                OpKind::Map(_, v) => *v,
                OpKind::Str => OpKind::Int,
                OpKind::Pointer(inner) => match *inner {
                    OpKind::Array(e) => *e,
                    _ => OpKind::Unknown,
                },
                _ => OpKind::Unknown,
            },
            Expr::Slice { base, .. } => match self.kind_of_expr(base) {
                OpKind::Slice(e) | OpKind::Array(e) => OpKind::Slice(e),
                OpKind::Str => OpKind::Str,
                OpKind::Pointer(inner) => match *inner {
                    OpKind::Array(e) => OpKind::Slice(e),
                    _ => OpKind::Unknown,
                },
                _ => OpKind::Unknown,
            },
            Expr::Field { base, name, .. } => {
                let bk = self.kind_of_expr(base);
                self.struct_field_kind(&bk, name).unwrap_or(OpKind::Unknown)
            }
            Expr::StructLit { ty, .. } => ty
                .as_ref()
                .map(|t| self.kind_of_ty(t, 0))
                .unwrap_or(OpKind::Struct(None)),
            Expr::SliceLit { elem, items, .. } => {
                if let Some((_, inner_elem, _)) = e.as_array_lit() {
                    return OpKind::Array(Box::new(self.kind_of_ty(inner_elem, 0)));
                }
                let ek = elem
                    .as_ref()
                    .map(|t| self.kind_of_ty(t, 0))
                    .or_else(|| items.first().map(|i| self.kind_of_expr(i)))
                    .unwrap_or(OpKind::Unknown);
                OpKind::Slice(Box::new(ek))
            }
            Expr::MapLit { key, value, .. } => OpKind::Map(
                Box::new(self.kind_of_ty(key, 0)),
                Box::new(self.kind_of_ty(value, 0)),
            ),
            Expr::FnLit {
                params, results, ..
            } => OpKind::Fn(
                results
                    .iter()
                    .map(|r| self.kind_of_ty(&r.ty, 0))
                    .collect(),
                Some(params.len()),
            ),
            Expr::Call { callee, args, .. } => match self.call_result_kinds(callee, args) {
                Some(res) if res.is_empty() => OpKind::Void,
                Some(res) => res[0].clone(),
                None => OpKind::Unknown,
            },
        }
    }

    fn call_result_kinds(&self, callee: &Expr, args: &[CallArg]) -> Option<Vec<OpKind>> {
        let bare = unwrap_parens(callee);
        match bare {
            Expr::FnLit { results, .. } => Some(
                results
                    .iter()
                    .map(|r| self.kind_of_ty(&r.ty, 0))
                    .collect(),
            ),
            Expr::SliceLit {
                elem: Some(t),
                items,
                ..
            } if items.is_empty() && bare.as_array_lit().is_none() => {
                Some(vec![OpKind::Slice(Box::new(self.kind_of_ty(t, 0)))])
            }
            Expr::Ident { name, .. } => {
                if let Some(k) = self.lookup_kind(name) {
                    if let OpKind::Fn(res, _) = k {
                        return Some(res);
                    }
                    return None;
                }
                match name.as_str() {
                    "len" | "cap" => return Some(vec![OpKind::Int]),
                    "append" => {
                        let first = args
                            .first()
                            .map(|a| self.kind_of_expr(&a.value))
                            .unwrap_or_else(|| OpKind::Slice(Box::new(OpKind::Unknown)));
                        return Some(vec![first]);
                    }
                    "make" => return Some(vec![self.make_kind(args)]),
                    "new" => return Some(vec![OpKind::Pointer(Box::new(OpKind::Unknown))]),
                    "delete" | "close" => return Some(Vec::new()),
                    _ => {}
                }
                let conv = self.kind_of_named(name, 0);
                if !matches!(conv, OpKind::Unknown | OpKind::Struct(_)) {
                    return Some(vec![conv]);
                }
                let head = Checker::base_named_type_str(name);
                if let Some(f) = self.ck.fns.get(name.as_str()).or_else(|| self.ck.fns.get(head)) {
                    return Some(
                        f.results
                            .iter()
                            .map(|r| self.kind_of_ty(&r.ty, 0))
                            .collect(),
                    );
                }
                None
            }
            Expr::Field { base, name, .. } => {
                let bk = self.kind_of_expr(base);
                if name == "pop" && args.is_empty() {
                    if let OpKind::Slice(elem) = bk {
                        return Some(vec![*elem]);
                    }
                }
                let head = Checker::base_named_type_str(name);
                if let Some(sname) = self.struct_base_name(&bk) {
                    if let Some(m) = self.ck.method_decls.get(&(sname, head.to_string())) {
                        return Some(
                            m.results
                                .iter()
                                .map(|r| self.kind_of_ty(&r.ty, 0))
                                .collect(),
                        );
                    }
                }
                if let Some(OpKind::Fn(res, _)) = self.struct_field_kind(&bk, name) {
                    return Some(res);
                }
                None
            }
            _ => None,
        }
    }

    fn make_kind(&self, args: &[CallArg]) -> OpKind {
        let Some(first) = args.first() else {
            return OpKind::Unknown;
        };
        let bare = unwrap_parens(&first.value);
        match bare {
            Expr::StructLit { ty: Some(t), .. } => self.kind_of_ty(t, 0),
            Expr::SliceLit { elem: Some(t), .. } if bare.as_array_lit().is_none() => {
                OpKind::Slice(Box::new(self.kind_of_ty(t, 0)))
            }
            Expr::MapLit { key, value, .. } => OpKind::Map(
                Box::new(self.kind_of_ty(key, 0)),
                Box::new(self.kind_of_ty(value, 0)),
            ),
            Expr::Ident { name, .. } => self.kind_of_named(name, 0),
            _ => OpKind::Unknown,
        }
    }

    fn emit_e1002(&mut self, span: Span, msg: String, note: &str) {
        self.diags.push(
            Diagnostic::error("HYB-E1002", self.path.clone(), span, msg)
                .with_notes(vec![note.to_string()]),
        );
    }

    fn emit_e2005(&mut self, span: Span, msg: String, note: &str) {
        self.diags.push(
            Diagnostic::error("HYB-E2005", self.path.clone(), span, msg)
                .with_notes(vec![note.to_string()]),
        );
    }

    /// #682 U14/U15 — a struct literal may only name fields the struct
    /// declares, once each, with a value assignable to the field's type. Go
    /// refuses the emitted literal otherwise (`unknown field`, `duplicate
    /// field name`, `cannot use ... as ... value in struct literal`).
    ///
    /// Fail-open everywhere the kind model cannot prove a mismatch: an
    /// unresolved struct name, an `Unknown` value kind, and an `Unknown`
    /// field type all stay silent.
    fn check_struct_literal(
        &mut self,
        ty: Option<&TypeExpr>,
        fields: &[FieldInit],
        span: Span,
    ) {
        let Some(t) = ty else { return };
        let sk = self.kind_of_ty(t, 0);
        let Some(full_name) = self.struct_name(&sk) else {
            return;
        };
        let sname = Checker::base_named_type_str(&full_name).to_string();
        // Clone: the diagnostics below need `&mut self` while the declaration
        // table is borrowed from it.
        let Some(declared) = self.ck.structs.get(&sname).cloned() else {
            return;
        };
        let subst = self.struct_type_arg_map(&full_name);

        // U15: a name repeated in one literal. Reported once per name, on the
        // second and later occurrences.
        let mut seen: HashMap<String, usize> = HashMap::new();
        for f in fields {
            let n = seen.entry(f.name.clone()).or_insert(0);
            *n += 1;
            if *n == 2 {
                self.emit_e1002(
                    f.span,
                    format!(
                        "duplicate field `{}` in `{}` literal — Go refuses `duplicate field name {} in struct literal` (HYB-E1002)",
                        f.name, full_name, f.name
                    ),
                    "each field may be set at most once in one struct literal",
                );
            }
        }

        // U14 + value type: unknown names, then assignability.
        for f in fields {
            match declared.iter().find(|(n, _)| n == &f.name) {
                None => self.emit_e1002(
                    f.span,
                    format!(
                        "unknown field `{}` in `{}` literal — Go refuses `unknown field {} in struct literal of type {}` (HYB-E1002)",
                        f.name, full_name, f.name, full_name
                    ),
                    "a struct literal may only name fields the struct declares",
                ),
                Some((_, fty)) => {
                    let want = self.kind_of_ty_with_subst(fty, &subst, 0);
                    let got = self.kind_of_expr(&f.value);
                    if kinds_conflict(&want, &got) {
                        self.emit_e1002(
                            f.value.span(),
                            format!(
                                "cannot use a value of type `{}` for field `{}` of `{}` (declared `{}`) — Go refuses `cannot use ... as {} value in struct literal` (HYB-E1002)",
                                got.label(),
                                f.name,
                                full_name,
                                want.label(),
                                want.label()
                            ),
                            "the field's declared type is fixed; write an explicit conversion when a conversion is intended",
                        );
                    }
                }
            }
        }
        let _ = span;
    }

    /// #682 U13 — a type conversion takes exactly one argument. Go refuses
    /// `int(1, 2)` with `too many arguments in conversion to int`.
    ///
    /// Only fires for a bare identifier that resolves to a KNOWN type. That
    /// check does the filtering for free: `kind_of_named("int")` is `Int`,
    /// while a builtin name (`len`, `append`, `make`, ...) is not a type and
    /// resolves to `Unknown`, so builtins keep their own arity rules
    /// (`builtins.rs`, `HYB-E2005`) and user functions keep `check_user_fn_arity`.
    fn check_conversion_arity(&mut self, name: &str, args: &[CallArg], span: Span) {
        let k = self.kind_of_named(name, 0);
        if matches!(k, OpKind::Unknown) {
            return;
        }
        if args.len() == 1 {
            return;
        }
        self.emit_e1002(
            span,
            format!(
                "a conversion to `{}` takes exactly 1 argument, found {} — Go refuses `too many arguments in conversion to {}` (HYB-E1002)",
                name,
                args.len(),
                name
            ),
            "a type conversion has the form `T(x)`: exactly one argument",
        );
    }

    /// #682 U6/U8 — the number of values a call expression produces, when it is
    /// knowable and is NOT exactly one.
    ///
    /// `Some(0)` is a call that produces no value (`fn v()`), which Go refuses
    /// with `v() (no value) used as value`. `Some(n)` for `n >= 2` is a
    /// multi-value call, which Go refuses in any single-value position with
    /// `multiple-value f() (value of type (T, U)) in single-value context`.
    ///
    /// `None` means "one value, or unknown" — the caller must stay permissive.
    /// This is the single place the multi-value shape is derived, so the
    /// distinction is not re-guessed at each call site.
    fn non_single_value_count(&self, e: &Expr) -> Option<usize> {
        match unwrap_parens(e) {
            Expr::Call { callee, args, .. } => {
                let res = self.call_result_kinds(callee, args)?;
                if res.len() == 1 {
                    None
                } else {
                    Some(res.len())
                }
            }
            _ => None,
        }
    }

    /// Reject a call used where exactly one value is required (#682 U6/U8).
    ///
    /// `binding_position` says whether the surrounding position is a binding or
    /// assignment target. **HYB-E2002 (T23, SPEC-LANGUAGE §2.8) already owns
    /// the multi-value case there** — `p := minmax(3, 9)` is E2002's own
    /// spec-cited claim, and `multi_value_tuple.rs` asserts that shape yields
    /// exactly one diagnostic. So in a binding position this rule reports only
    /// the VOID case, which E2002 does not cover, and leaves multi-value to
    /// E2002. In every other position (operands, arguments) no rule covers
    /// either shape, so both are reported here.
    fn check_single_value_position(&mut self, e: &Expr, context: &str, binding_position: bool) {
        match self.non_single_value_count(e) {
            Some(0) => self.emit_e1002(
                e.span(),
                format!(
                    "`{}` produces no value and cannot be used as one in {context} — Go refuses `{} (no value) used as value` (HYB-E1002)",
                    e.render(),
                    e.render()
                ),
                "a function declared with no results is a statement, not a value; call it on its own line",
            ),
            Some(n) => {
                if binding_position {
                    // E2002 owns this shape; do not double-report.
                    return;
                }
                self.emit_e1002(
                    e.span(),
                    format!(
                        "`{}` produces {n} values and cannot be used as one in {context} — Go refuses `multiple-value ... in single-value context` (HYB-E1002)",
                        e.render()
                    ),
                    "bind the results first (`a, b := f()`) or select the one you need",
                );
            }
            None => {}
        }
    }

    /// #682 U7 — a multi-value call as the whole argument list is legal Go when
    /// the callee takes exactly that many parameters (`g(two())` for a 2-param
    /// `g`), so the arity rule has to accept that case rather than treat it as
    /// one argument.
    fn multi_value_arg_count(&self, args: &[CallArg]) -> Option<usize> {
        if args.len() != 1 {
            return None;
        }
        match self.non_single_value_count(&args[0].value) {
            Some(n) if n >= 2 => Some(n),
            _ => None,
        }
    }

    /// #577 C9 — a closure or `fn`-typed value whose declared parameter count
    /// is known (`f := fn(x int) int {...}; f(1, 2)`).
    fn report_local_arity(&mut self, name: &str, want: usize, got: usize, span: Span) {
        let msg = if got < want {
            format!(
                "`{}` takes {} argument{} but is called with {} — Go refuses `not enough arguments in call to {}` (HYB-E1002)",
                name,
                want,
                if want == 1 { "" } else { "s" },
                got,
                name
            )
        } else {
            format!(
                "`{}` takes {} argument{} but is called with {} — Go refuses `too many arguments in call to {}` (HYB-E1002)",
                name,
                want,
                if want == 1 { "" } else { "s" },
                got,
                name
            )
        };
        self.emit_e1002(
            span,
            msg,
            "a call must pass one argument per declared parameter; the emitted Go call is rejected at build time",
        );
    }

    /// #577 C9 — a call to a top-level `fn` must pass exactly as many
    /// arguments as the declaration has parameters. Go refuses the emitted
    /// call otherwise (`not enough arguments` / `too many arguments`), so an
    /// accepted program would not build (Row 13).
    ///
    /// Only fires for a bare identifier that resolves to a declared `fn`.
    /// A name bound to a local (including a fn-typed variable or closure) is
    /// skipped by the caller — the local's arity is not tracked here — and a
    /// builtin never reaches this point because builtins are not in `fns`.
    fn check_user_fn_arity(&mut self, name: &str, args: &[CallArg], span: Span) {
        let head = Checker::base_named_type_str(name);
        let Some(f) = self.ck.fns.get(name).or_else(|| self.ck.fns.get(head)) else {
            return;
        };
        // Generic instantiations (`id[int](x)`) carry type arguments the
        // parameter list does not describe; leave them to the type checker.
        if !f.type_params.is_empty() {
            return;
        }
        // A lone multi-value call IS the whole argument list and expands to one
        // argument per result: `g(two())` for a 2-param `g` is one written
        // argument but two effective ones, and it builds. #682 U7's
        // `check_multi_value_arg_arity` owns exactly that comparison, so
        // counting the written `args.len()` here would reject buildable Go.
        if self.multi_value_arg_count(args).is_some() {
            return;
        }
        let (want, got) = (f.params.len(), args.len());
        if want == got {
            return;
        }
        let msg = if got < want {
            format!(
                "`{}` takes {} argument{} but is called with {} — Go refuses `not enough arguments in call to {}` (HYB-E1002)",
                name,
                want,
                if want == 1 { "" } else { "s" },
                got,
                name
            )
        } else {
            format!(
                "`{}` takes {} argument{} but is called with {} — Go refuses `too many arguments in call to {}` (HYB-E1002)",
                name,
                want,
                if want == 1 { "" } else { "s" },
                got,
                name
            )
        };
        self.emit_e1002(
            span,
            msg,
            "a call must pass one argument per declared parameter; the emitted Go call is rejected at build time",
        );
    }

    /// #577 C9 — method calls (`x.bump(k)`) declare their receiver
    /// separately, so the parameter count is the declared one.
    fn check_method_arity(&mut self, bk: &OpKind, name: &str, args: &[CallArg], span: Span) {
        let Some(sn) = self.struct_name(bk) else {
            return;
        };
        let Some(m) = self.ck.method_decls.get(&(sn.clone(), name.to_string())) else {
            return;
        };
        if !m.type_params.is_empty() {
            return;
        }
        let (want, got) = (m.params.len(), args.len());
        if want == got {
            return;
        }
        let msg = if got < want {
            format!(
                "method `{}` on `{}` takes {} argument{} but is called with {} — Go refuses `not enough arguments in call to {}.{}` (HYB-E1002)",
                name,
                sn,
                want,
                if want == 1 { "" } else { "s" },
                got,
                sn,
                name
            )
        } else {
            format!(
                "method `{}` on `{}` takes {} argument{} but is called with {} — Go refuses `too many arguments in call to {}.{}` (HYB-E1002)",
                name,
                sn,
                want,
                if want == 1 { "" } else { "s" },
                got,
                sn,
                name
            )
        };
        self.emit_e1002(
            span,
            msg,
            "the receiver is not an argument: pass one argument per declared parameter",
        );
    }

    fn check_user_fn_arg_types(&mut self, name: &str, args: &[CallArg]) {
        let head = Checker::base_named_type_str(name);
        let Some(f) = self.ck.fns.get(name).or_else(|| self.ck.fns.get(head)) else {
            return;
        };
        if !f.type_params.is_empty() {
            return;
        }
        if self.multi_value_arg_count(args).is_some() {
            return;
        }
        if f.params.len() != args.len() {
            return;
        }
        for (p, a) in f.params.iter().zip(args.iter()) {
            let want = self.kind_of_ty(&p.ty, 0);
            let got = self.kind_of_expr(&a.value);
            if let OpKind::Pointer(inner) = &want {
                if !kinds_conflict(inner, &got) && self.is_addressable_value_receiver(&a.value) {
                    continue;
                }
            }
            if kinds_conflict(&want, &got) {
                self.emit_e1002(
                    a.value.span(),
                    format!(
                        "cannot use `{}` (value of type `{}`) as `{}` value in argument to `{}` — Go refuses mismatched argument types (HYB-E1002)",
                        a.value.render(),
                        got.label(),
                        want.label(),
                        name
                    ),
                    "argument types must match declared parameter types",
                );
            }
        }
    }

    fn check_method_arg_types(&mut self, bk: &OpKind, name: &str, args: &[CallArg]) {
        let Some(sn) = self.struct_base_name(bk).or_else(|| self.struct_name(bk)) else {
            return;
        };
        let Some(m) = self.ck.method_decls.get(&(sn.clone(), name.to_string())) else {
            return;
        };
        if !m.type_params.is_empty() {
            return;
        }
        if self.multi_value_arg_count(args).is_some() {
            return;
        }
        if m.params.len() != args.len() {
            return;
        }
        for (p, a) in m.params.iter().zip(args.iter()) {
            let want = self.kind_of_ty(&p.ty, 0);
            let got = self.kind_of_expr(&a.value);
            if let OpKind::Pointer(inner) = &want {
                if !kinds_conflict(inner, &got) && self.is_addressable_value_receiver(&a.value) {
                    continue;
                }
            }
            if kinds_conflict(&want, &got) {
                self.emit_e1002(
                    a.value.span(),
                    format!(
                        "cannot use `{}` (value of type `{}`) as `{}` value in argument to `{}.{}` — Go refuses mismatched argument types (HYB-E1002)",
                        a.value.render(),
                        got.label(),
                        want.label(),
                        sn,
                        name
                    ),
                    "argument types must match declared parameter types",
                );
            }
        }
    }

    /// #577 C10 — a `return` must supply the enclosing function's result
    /// list: the same number of values, each assignable to its result type.
    /// Go refuses the emitted statement otherwise (Row 13).
    fn check_return(&mut self, values: &[Expr], span: Span) {
        let want = self.fn_results.clone();
        // Go's multi-value rule: `return f()` where `f`'s results line up
        // exactly with the enclosing signature. The single call is the whole
        // value list, so compare its results positionally instead of
        // rejecting the count.
        if values.len() == 1 {
            if let Expr::Call { callee, args, .. } = unwrap_parens(&values[0]) {
                if let Some(res) = self.call_result_kinds(callee, args) {
                    if res.len() == want.len() {
                        for (r, w) in res.iter().zip(want.iter()) {
                            self.report_return_conflict(r, w, values[0].span());
                        }
                        return;
                    }
                }
            }
        }
        if values.len() != want.len() {
            let (nv, nw) = (values.len(), want.len());
            // Fewer values than results is legal in exactly two shapes, and
            // both are lowered to a full-arity Go return:
            //   * every result is named — Go's naked `return` (corpus 63);
            //   * the last result is `error` — the emitter's E3 zero-fill
            //     turns `return v` into `return v, nil` (corpus 13).
            // Anything else is a genuine arity error.
            let short_ok = nv < nw
                && ((nv == 0 && self.fn_results_all_named) || self.fn_results_error_tail);
            if short_ok {
                for (v, w) in values.iter().zip(want.iter()) {
                    let got = self.kind_of_expr(v);
                    self.report_return_conflict(&got, w, v.span());
                }
                return;
            }
            let msg = match (nw, nv) {
                (0, _) => format!(
                    "`return` with {nv} value{} in a function that declares no results — Go refuses `too many return values` (HYB-E1002)",
                    if nv == 1 { "" } else { "s" }
                ),
                (_, 0) => format!(
                    "`return` without a value in a function that declares {nw} result{} — Go refuses `not enough return values` (HYB-E1002)",
                    if nw == 1 { "" } else { "s" }
                ),
                _ => format!(
                    "`return` supplies {nv} value{} but the signature declares {nw} result{} — Go refuses `not enough return values` / `too many return values` (HYB-E1002)",
                    if nv == 1 { "" } else { "s" },
                    if nw == 1 { "" } else { "s" }
                ),
            };
            self.emit_e1002(
                span,
                msg,
                "a return statement must supply exactly the results the signature declares",
            );
            return;
        }
        for (v, w) in values.iter().zip(want.iter()) {
            let got = self.kind_of_expr(v);
            self.report_return_conflict(&got, w, v.span());
        }
    }

    fn report_return_conflict(&mut self, got: &OpKind, want: &OpKind, span: Span) {
        if !kinds_conflict(want, got) {
            return;
        }
        self.emit_e1002(
            span,
            format!(
                "cannot return a value of type `{}` from a function declaring `{}` — Go refuses `cannot use ... as {} value in return statement` (HYB-E1002)",
                got.label(),
                want.label(),
                want.label()
            ),
            "a returned value must be assignable to its declared result type; write an explicit conversion when a conversion is intended",
        );
    }

    /// #682 U7 — a lone multi-value call as the whole argument list is legal
    /// only when the callee takes exactly that many parameters.
    fn check_multi_value_arg_arity(&mut self, callee: &Expr, args: &[CallArg], span: Span) {
        let Some(n) = self.multi_value_arg_count(args) else {
            return;
        };
        let Expr::Ident { name, .. } = unwrap_parens(callee) else {
            return;
        };
        let head = Checker::base_named_type_str(name);
        let Some(f) = self.ck.fns.get(name).or_else(|| self.ck.fns.get(head)) else {
            return;
        };
        let want = f.params.len();
        if want == n {
            return;
        }
        self.emit_e1002(
            span,
            format!(
                "`{}` takes {want} argument{} but its argument list expands to {n} value{} — Go refuses `{} arguments in call to {}` (HYB-E1002)",
                name,
                if want == 1 { "" } else { "s" },
                if n == 1 { "" } else { "s" },
                if n < want { "not enough" } else { "too many" },
                name
            ),
            "a multi-value call expands to one argument per result; the callee must accept exactly that many",
        );
    }

    /// #682 U5 — `append`'s appended values must be assignable to the slice's
    /// element type. Go refuses `append([]int{1}, "str")` with
    /// `cannot use "str" (untyped string constant) as int value in argument to
    /// append`.
    ///
    /// Fail-open: an `Unknown` first argument or an `Unknown` element kind
    /// carries no claim, so only a proven mismatch is reported.
    fn check_append_elements(&mut self, args: &[CallArg]) {
        let Some(first) = args.first() else { return };
        let elem = match self.kind_of_expr(&first.value) {
            OpKind::Slice(e) => *e,
            _ => return,
        };
        for extra in &args[1..] {
            // `append(a, b...)` — a variadic spread passes the whole slice, so
            // the argument's kind is the SLICE type, not an element. Go accepts
            // it (for a matching element type), so it must not be compared
            // against the element kind.
            if extra.spread {
                continue;
            }
            let got = self.kind_of_expr(&extra.value);
            if kinds_conflict(&elem, &got) {
                self.emit_e1002(
                    extra.value.span(),
                    format!(
                        "cannot append a value of type `{}` to a `{}` — Go refuses `cannot use ... as {} value in argument to append` (HYB-E1002)",
                        got.label(),
                        self.kind_of_expr(&first.value).label(),
                        elem.label()
                    ),
                    "the appended values must be assignable to the slice's element type",
                );
            }
        }
    }

    /// #682 U4 — `defer` requires a call, and that call's arguments must
    /// satisfy the callee's arity. Go refuses `defer g()` for a 1-param `g`
    /// with `not enough arguments in call to g`.
    ///
    /// `builtins.rs::check_stmt_defer_call` owns the "must be a call" half
    /// (HYB-E2001); this owns the arity half, and only for a bare identifier
    /// that resolves to a declared `fn`, so an unresolved or shadowed name
    /// stays fail-open.
    fn check_defer_arity(&mut self, call: &Expr, span: Span) {
        let mut e = unwrap_parens(call);
        if let Expr::Try { operand, .. } = e {
            e = unwrap_parens(operand);
        }
        let Expr::Call { callee, args, .. } = e else {
            return;
        };
        let Expr::Ident { name, .. } = unwrap_parens(callee) else {
            return;
        };
        let head = Checker::base_named_type_str(name);
        let Some(f) = self.ck.fns.get(name).or_else(|| self.ck.fns.get(head)) else {
            return;
        };
        if !f.type_params.is_empty() || f.params.len() == args.len() {
            return;
        }
        let want = f.params.len();
        let got = args.len();
        self.emit_e1002(
            span,
            format!(
                "`defer {}()` passes {got} argument{} but `{}` takes {want} — Go refuses `{} arguments in call to {}` (HYB-E1002)",
                name,
                if got == 1 { "" } else { "s" },
                name,
                if got < want { "not enough" } else { "too many" },
                name
            ),
            "the deferred call is checked like any other call: one argument per declared parameter",
        );
    }

    /// Family A (#425): `if cond` and `for cond` require a boolean condition.
    fn check_bool_condition(&mut self, cond: &Expr, construct: &str) {
        let k = self.kind_of_expr(cond);
        if !matches!(k, OpKind::Unknown | OpKind::Bool) {
            self.emit_e1002(
                cond.span(),
                format!(
                    "non-boolean condition `{}` (type `{}`) in `{}` statement — Go requires a `bool` condition (HYB-E1002)",
                    cond.render(),
                    k.label(),
                    construct
                ),
                "compare the value explicitly (e.g. `x != 0`, `len(s) > 0`) or pass a `bool` expression",
            );
        }
    }

    /// Family B1/B2 (#425): unary `!` requires `bool`, unary `-` requires a numeric operand.
    fn check_unary_operand(&mut self, op: UnOp, operand: &Expr, span: Span) {
        let k = self.kind_of_expr(operand);
        match op {
            UnOp::Not => {
                if !matches!(k, OpKind::Unknown | OpKind::Bool) {
                    self.emit_e1002(
                        span,
                        format!(
                            "operator `!` not defined on `{}` (type `{}`) — `!` requires a `bool` operand (HYB-E1002)",
                            operand.render(),
                            k.label()
                        ),
                        "compare numeric or string values explicitly (e.g. `x == 0`, `s == \"\"`)",
                    );
                }
            }
            UnOp::Neg => {
                if !matches!(k, OpKind::Unknown | OpKind::Int | OpKind::Float) {
                    self.emit_e1002(
                        span,
                        format!(
                            "operator `-` not defined on `{}` (type `{}`) — unary `-` requires a numeric operand (HYB-E1002)",
                            operand.render(),
                            k.label()
                        ),
                        "unary `-` is only defined on integer and floating-point values",
                    );
                }
            }
            UnOp::Addr => {}
        }
    }

    /// Family B3/B4/B5 (#425): binary operator operand-kind checks.
    fn check_binary_operands(&mut self, op: BinOp, lhs: &Expr, rhs: &Expr, span: Span) {
        // #682 U6: an operand must be exactly one value. `two() + 1` is Go's
        // `multiple-value two() (value of type (int, int)) in single-value
        // context`, and the kind inference below would otherwise take the
        // call's FIRST result and silently accept it as an int.
        self.check_single_value_position(lhs, "a binary operation", /*binding_position=*/ false);
        self.check_single_value_position(rhs, "a binary operation", /*binding_position=*/ false);
        let lk = self.kind_of_expr(lhs);
        let rk = self.kind_of_expr(rhs);
        match op {
            // `==` / `!=`: incomparable shapes (slice, map, func) are rejected
            // by `emit_guard.rs` (HYB-E2006). Mismatched concrete operand types
            // are rejected here under HYB-E1002 (rule `BUILDABLE`).
            BinOp::Eq | BinOp::Ne => {
                let op_str = if matches!(op, BinOp::Eq) { "==" } else { "!=" };
                if !Checker::is_nil_literal(lhs)
                    && !Checker::is_nil_literal(rhs)
                    && equality_kinds_conflict(&lk, &rk)
                {
                    self.emit_e1002(
                        span,
                        format!(
                            "invalid operation `{}`: operator `{}` requires comparable operands of matching type, found `{}` and `{}` (HYB-E1002)",
                            span_expr_label(lhs, op_str, rhs),
                            op_str,
                            lk.label(),
                            rk.label()
                        ),
                        "equality operators (`==`, `!=`) require operands of matching type",
                    );
                }
            }
            // B4: `&&` and `||` require `bool` operands.
            BinOp::And | BinOp::Or => {
                let op_str = if matches!(op, BinOp::And) { "&&" } else { "||" };
                if !matches!(lk, OpKind::Unknown | OpKind::Bool)
                    || !matches!(rk, OpKind::Unknown | OpKind::Bool)
                {
                    self.emit_e1002(
                        span,
                        format!(
                            "invalid operation `{}`: operator `{}` requires `bool` operands, found `{}` and `{}` (HYB-E1002)",
                            span_expr_label(lhs, op_str, rhs),
                            op_str,
                            lk.label(),
                            rk.label()
                        ),
                        "both sides of `&&` and `||` must evaluate to `bool`",
                    );
                }
            }
            // B5: ordered comparisons `<`, `<=`, `>`, `>=` are defined only on numeric or string operands.
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                let op_str = match op {
                    BinOp::Lt => "<",
                    BinOp::Le => "<=",
                    BinOp::Gt => ">",
                    _ => ">=",
                };
                let l_ok = matches!(lk, OpKind::Unknown | OpKind::Int | OpKind::Float | OpKind::Str);
                let r_ok = matches!(rk, OpKind::Unknown | OpKind::Int | OpKind::Float | OpKind::Str);
                let mismatch = lk != OpKind::Unknown
                    && rk != OpKind::Unknown
                    && lk.is_numeric() != rk.is_numeric();
                if !l_ok || !r_ok || mismatch {
                    self.emit_e1002(
                        span,
                        format!(
                            "operator `{}` not defined on `{}` and `{}` in `{}` (HYB-E1002)",
                            op_str,
                            lk.label(),
                            rk.label(),
                            span_expr_label(lhs, op_str, rhs)
                        ),
                        "ordered comparisons (`<`, `<=`, `>`, `>=`) require two numeric values or two strings",
                    );
                }
            }
            // B3: `+` accepts numeric+numeric or string+string.
            BinOp::Add => {
                let l_ok = matches!(lk, OpKind::Unknown | OpKind::Int | OpKind::Float | OpKind::Str);
                let r_ok = matches!(rk, OpKind::Unknown | OpKind::Int | OpKind::Float | OpKind::Str);
                let mismatch = lk != OpKind::Unknown
                    && rk != OpKind::Unknown
                    && lk.is_numeric() != rk.is_numeric();
                if !l_ok || !r_ok || mismatch {
                    self.emit_e1002(
                        span,
                        format!(
                            "invalid operation `{}` (mismatched or non-addable types `{}` and `{}`) (HYB-E1002)",
                            span_expr_label(lhs, "+", rhs),
                            lk.label(),
                            rk.label()
                        ),
                        "`+` requires two numeric operands or two strings; use string interpolation `\"{x}{s}\"` to format mixed values",
                    );
                }
            }
            // `-`, `*`, `/` require numeric operands.
            BinOp::Sub | BinOp::Mul | BinOp::Div => {
                let op_str = match op {
                    BinOp::Sub => "-",
                    BinOp::Mul => "*",
                    _ => "/",
                };
                if !matches!(lk, OpKind::Unknown | OpKind::Int | OpKind::Float)
                    || !matches!(rk, OpKind::Unknown | OpKind::Int | OpKind::Float)
                {
                    self.emit_e1002(
                        span,
                        format!(
                            "invalid operation `{}`: operator `{}` not defined on `{}` and `{}` (HYB-E1002)",
                            span_expr_label(lhs, op_str, rhs),
                            op_str,
                            lk.label(),
                            rk.label()
                        ),
                        "arithmetic `-`, `*`, `/` require numeric operands",
                    );
                }
            }
            // `%`, bitwise, and shifts require integer operands.
            BinOp::Mod
            | BinOp::BitOr
            | BinOp::BitXor
            | BinOp::BitAnd
            | BinOp::Shl
            | BinOp::Shr => {
                let op_str = match op {
                    BinOp::Mod => "%",
                    BinOp::BitOr => "|",
                    BinOp::BitXor => "^",
                    BinOp::BitAnd => "&",
                    BinOp::Shl => "<<",
                    _ => ">>",
                };
                if !matches!(lk, OpKind::Unknown | OpKind::Int)
                    || !matches!(rk, OpKind::Unknown | OpKind::Int)
                {
                    self.emit_e1002(
                        span,
                        format!(
                            "invalid operation `{}`: operator `{}` requires integer operands, found `{}` and `{}` (HYB-E1002)",
                            span_expr_label(lhs, op_str, rhs),
                            op_str,
                            lk.label(),
                            rk.label()
                        ),
                        "modulo, bitwise, and shift operators require integer operands",
                    );
                }
            }
        }
    }

    /// Family B6 (#425): `s++`, `s--`, and compound assignment operand checks.
    fn check_assign_operands(
        &mut self,
        targets: &[Place],
        op: AssignOp,
        values: &[Expr],
        span: Span,
    ) {
        let Some(target) = targets.first() else {
            return;
        };
        let tk = self.kind_of_place(target);
        if op != AssignOp::Assign && op != AssignOp::Inc && op != AssignOp::Dec {
            if targets.len() != 1 || values.len() != 1 {
                self.emit_e1002(
                    span,
                    "compound assignment operator does not support multiple targets or values (HYB-E1002)".to_string(),
                    "compound assignment operators (`+=`, `-=`, etc.) require exactly one target and one value; write separate statements or use `=` for parallel assignment",
                );
                return;
            }
        }
        match op {
            // #682 U16: plain `=` requires the value to be assignable to the
            // target. Only the single-target/single-value form is checked
            // positionally; a multi-value right-hand side (`a, b = f()`) is
            // left to the call-shape rules.
            AssignOp::Assign => {
                if targets.len() == 1 && values.len() == 1 {
                    // O8 slot-loan (adapted during integration): `q = &m["a"]`
                    // on a value-typed `q` does NOT emit `q = &m["a"]` — the
                    // emitter lowers the loan to `q = m["a"]` plus a store-back
                    // into the slot, so a `Struct` target receiving a
                    // `Pointer` value here is legal Hybrid, not a mismatch
                    // (map_element_addressable's
                    // accepts_slot_loan_into_value_var pins the shape).
                    // Fail open for exactly that shape: an addr-of whose
                    // operand is an index or field place. Loans are validated
                    // for addressability by addressable.rs (HYB-E2010), so
                    // the non-addressable ones still reject.
                    let is_loan_shape = as_addr_of(&values[0])
                        .is_some_and(|o| matches!(o, Expr::Index { .. } | Expr::Field { .. }));
                    let vk = self.kind_of_expr(&values[0]);
                    if !is_loan_shape && kinds_conflict(&tk, &vk) {
                        self.emit_e1002(
                            span,
                            format!(
                                "cannot assign a value of type `{}` to `{}` (type `{}`) — Go refuses `cannot use ... as {} value in assignment` (HYB-E1002)",
                                vk.label(),
                                target.render(),
                                tk.label(),
                                tk.label()
                            ),
                            "the target's declared type is fixed; write an explicit conversion when a conversion is intended",
                        );
                    }
                }
            }
            AssignOp::Inc | AssignOp::Dec => {
                let op_str = if matches!(op, AssignOp::Inc) { "++" } else { "--" };
                if !matches!(tk, OpKind::Unknown | OpKind::Int | OpKind::Float) {
                    self.emit_e1002(
                        span,
                        format!(
                            "invalid operation `{}{}` (non-numeric type `{}`) (HYB-E1002)",
                            target.render(),
                            op_str,
                            tk.label()
                        ),
                        "`++` and `--` require a numeric variable or place",
                    );
                }
            }
            AssignOp::AddAssign => {
                let Some(val) = values.first() else { return };
                let vk = self.kind_of_expr(val);
                let t_ok = matches!(tk, OpKind::Unknown | OpKind::Int | OpKind::Float | OpKind::Str);
                let v_ok = matches!(vk, OpKind::Unknown | OpKind::Int | OpKind::Float | OpKind::Str);
                let mismatch = tk != OpKind::Unknown
                    && vk != OpKind::Unknown
                    && tk.is_numeric() != vk.is_numeric();
                if !t_ok || !v_ok || mismatch || kinds_conflict(&tk, &vk) {
                    self.emit_e1002(
                        span,
                        format!(
                            "invalid operation `{} += {}` (mismatched or non-addable types `{}` and `{}`) (HYB-E1002)",
                            target.render(),
                            val.render(),
                            tk.label(),
                            vk.label()
                        ),
                        "`+=` requires two numeric values or two strings of matching types",
                    );
                }
            }
            AssignOp::SubAssign | AssignOp::MulAssign | AssignOp::DivAssign => {
                let Some(val) = values.first() else { return };
                let vk = self.kind_of_expr(val);
                let op_sym = match op {
                    AssignOp::SubAssign => "-=",
                    AssignOp::MulAssign => "*=",
                    _ => "/=",
                };
                if !matches!(tk, OpKind::Unknown | OpKind::Int | OpKind::Float)
                    || !matches!(vk, OpKind::Unknown | OpKind::Int | OpKind::Float)
                    || kinds_conflict(&tk, &vk)
                {
                    self.emit_e1002(
                        span,
                        format!(
                            "invalid compound assignment `{} {} {}` on `{}` and `{}` (HYB-E1002)",
                            target.render(),
                            op_sym,
                            val.render(),
                            tk.label(),
                            vk.label()
                        ),
                        "`-=`, `*=`, `/=` require numeric operands of matching types",
                    );
                }
            }
            AssignOp::ModAssign
            | AssignOp::BitOrAssign
            | AssignOp::BitAndAssign
            | AssignOp::BitXorAssign
            | AssignOp::ShlAssign
            | AssignOp::ShrAssign => {
                let Some(val) = values.first() else { return };
                let vk = self.kind_of_expr(val);
                if !matches!(tk, OpKind::Unknown | OpKind::Int)
                    || !matches!(vk, OpKind::Unknown | OpKind::Int)
                {
                    self.emit_e1002(
                        span,
                        format!(
                            "invalid integer compound assignment on `{}` (type `{}`) and `{}` (type `{}`) (HYB-E1002)",
                            target.render(),
                            tk.label(),
                            val.render(),
                            vk.label()
                        ),
                        "integer compound assignment operators require integer operands",
                    );
                }
            }
        }
    }

    /// Family C1 (#425, #708): `base[index]` requires a slice, array, map, or string,
    /// and constant indices on constant strings must satisfy `0 <= index < len(base)`.
    fn check_index_kind(
        &mut self,
        base_str: &str,
        base_expr: Option<&Expr>,
        bk: &OpKind,
        index: &Expr,
        ik: &OpKind,
        span: Span,
    ) {
        let indexable = matches!(
            bk,
            OpKind::Unknown
                | OpKind::Slice(_)
                | OpKind::Array(_)
                | OpKind::Map(_, _)
                | OpKind::Str
        ) || matches!(
            bk,
            OpKind::Pointer(inner) if matches!(**inner, OpKind::Unknown | OpKind::Array(_))
        );
        if !indexable {
            self.emit_e1002(
                span,
                format!(
                    "cannot index `{}` (type `{}`) — indexing requires a slice, array, map, or string (HYB-E1002)",
                    base_str,
                    bk.label()
                ),
                "only slices, arrays, maps, and strings can be indexed with `x[i]`",
            );
            return;
        }
        // For slices, arrays, and strings, the index must be an integer.
        let needs_int_idx = matches!(bk, OpKind::Slice(_) | OpKind::Array(_) | OpKind::Str)
            || matches!(bk, OpKind::Pointer(inner) if matches!(**inner, OpKind::Array(_)));
        if needs_int_idx && !matches!(ik, OpKind::Unknown | OpKind::Int) {
            self.emit_e1002(
                index.span(),
                format!(
                    "non-integer index `{}` (type `{}`) on `{}` (HYB-E1002)",
                    index.render(),
                    ik.label(),
                    bk.label()
                ),
                "slice, array, and string indices must be integers",
            );
        } else if needs_int_idx {
            if let Some(v) = self.eval_const_int_at(index, span.start) {
                if v < 0 {
                    self.emit_e1002(
                        index.span(),
                        format!(
                            "invalid index `{}` (constant `{}` must not be negative) on `{}` (HYB-E1002)",
                            index.render(),
                            v,
                            bk.label()
                        ),
                        "slice, array, and string indices must be non-negative integers",
                    );
                } else if matches!(bk, OpKind::Str) {
                    if let Some(str_len) =
                        base_expr.and_then(|b| self.eval_const_string_len_at(b, span.start))
                    {
                        if v >= str_len {
                            self.emit_e1002(
                                index.span(),
                                format!(
                                    "invalid string index `{}` (out of bounds for {}-byte string `{}`) (HYB-E1002)",
                                    index.render(),
                                    str_len,
                                    base_str
                                ),
                                "a constant index on a constant string must satisfy `0 <= index < len(s)`",
                            );
                        }
                    }
                }
            }
        }
    }

    /// Family C2 (#425, #708): `base[lo:hi]` requires a slice, array, or string,
    /// and constant bounds on constant strings must satisfy `0 <= lo <= hi <= len(base)`.
    fn check_slice_kind(
        &mut self,
        base: &Expr,
        bk: &OpKind,
        lo: Option<&Expr>,
        hi: Option<&Expr>,
        span: Span,
    ) {
        let sliceable = matches!(
            bk,
            OpKind::Unknown | OpKind::Slice(_) | OpKind::Array(_) | OpKind::Str
        ) || matches!(
            bk,
            OpKind::Pointer(inner) if matches!(**inner, OpKind::Unknown | OpKind::Array(_))
        );
        if !sliceable {
            self.emit_e1002(
                span,
                format!(
                    "cannot slice `{}` (type `{}`) — slicing requires a slice, array, or string (HYB-E1002)",
                    base.render(),
                    bk.label()
                ),
                "only slices, arrays, and strings support `x[lo:hi]` slicing",
            );
            return;
        }
        let const_str_len = if matches!(bk, OpKind::Str) {
            self.eval_const_string_len_at(base, span.start)
        } else {
            None
        };
        let mut lo_const: Option<i128> = None;
        let mut hi_const: Option<i128> = None;
        for (idx, bound_opt) in [lo, hi].into_iter().enumerate() {
            let Some(bound) = bound_opt else { continue };
            let k = self.kind_of_expr(bound);
            if !matches!(k, OpKind::Unknown | OpKind::Int) {
                self.emit_e1002(
                    bound.span(),
                    format!(
                        "non-integer slice bound `{}` (type `{}`) (HYB-E1002)",
                        bound.render(),
                        k.label()
                    ),
                    "slice bounds must be integers",
                );
            } else if let Some(v) = self.eval_const_int_at(bound, span.start) {
                if v < 0 {
                    self.emit_e1002(
                        bound.span(),
                        format!(
                            "invalid slice index `{}` (constant `{}` must not be negative) (HYB-E1002)",
                            bound.render(),
                            v
                        ),
                        "slice bounds must be non-negative integers",
                    );
                } else {
                    if let Some(str_len) = const_str_len {
                        if v > str_len {
                            self.emit_e1002(
                                bound.span(),
                                format!(
                                    "invalid string slice bound `{}` (out of bounds for {}-byte string `{}`) (HYB-E1002)",
                                    bound.render(),
                                    str_len,
                                    base.render()
                                ),
                                "a constant slice bound on a constant string must satisfy `0 <= bound <= len(s)`",
                            );
                        }
                    }
                    if idx == 0 {
                        lo_const = Some(v);
                    } else {
                        hi_const = Some(v);
                    }
                }
            }
        }
        if let (Some(lo_expr), Some(hi_expr), Some(lo_v), Some(hi_v)) =
            (lo, hi, lo_const, hi_const)
        {
            if lo_v > hi_v {
                self.emit_e1002(
                    span,
                    format!(
                        "invalid slice indices: `{}` > `{}` (`{} > {}`) — Go rejects inverted constant slice bounds at compile time (HYB-E1002)",
                        lo_expr.render(),
                        hi_expr.render(),
                        lo_v,
                        hi_v
                    ),
                    "the lower slice bound `lo` in `x[lo:hi]` must satisfy `lo <= hi`",
                );
            }
        }
    }

    fn is_non_const_local(&self, name: &str) -> bool {
        for layer in self.scopes.iter().rev() {
            if let Some(entry) = layer.get(name) {
                return match entry {
                    Some(id) => !matches!(self.defines[*id].kind, LocalKind::Const),
                    None => true,
                };
            }
        }
        false
    }

    fn expr_has_non_const_local(&self, e: &Expr) -> bool {
        match unwrap_parens(e) {
            Expr::Ident { name, .. } => self.is_non_const_local(name),
            Expr::Unary { operand, .. } => self.expr_has_non_const_local(operand),
            Expr::Binary { lhs, rhs, .. } => {
                self.expr_has_non_const_local(lhs) || self.expr_has_non_const_local(rhs)
            }
            Expr::Call { callee, args, .. } => {
                self.expr_has_non_const_local(callee)
                    || args.iter().any(|a| self.expr_has_non_const_local(&a.value))
            }
            _ => false,
        }
    }

    fn eval_const_int_at(&self, e: &Expr, use_pos: hy_diag::Pos) -> Option<i128> {
        if self.expr_has_non_const_local(e) {
            return None;
        }
        self.ck.const_int_value_at(e, use_pos, 0)
    }

    fn eval_const_string_len_at(&self, e: &Expr, use_pos: hy_diag::Pos) -> Option<i128> {
        if self.expr_has_non_const_local(e) {
            return None;
        }
        self.ck.const_string_len_at(e, use_pos, 0)
    }

    fn check_array_lit_items(&mut self, elem_ty: &TypeExpr, items: &[Expr]) {
        let resolved_ty = self.ck.resolve_type_alias(elem_ty);
        let expected = self.kind_of_ty(&resolved_ty, 0);
        if expected == OpKind::Unknown {
            return;
        }
        for item in items {
            let bare = unwrap_parens(item);
            if matches!(bare, Expr::Lit { kind: LitKind::Nil, .. }) {
                continue;
            }
            if let Expr::SliceLit {
                elem: None,
                items: sub_items,
                ..
            } = bare
            {
                if let TypeExpr::Array { elem: sub_ty, .. }
                | TypeExpr::Slice { elem: sub_ty, .. } = &resolved_ty
                {
                    self.check_array_lit_items(sub_ty, sub_items);
                    continue;
                }
            }
            if let Expr::StructLit {
                ty: None, fields, ..
            } = bare
            {
                if matches!(expected, OpKind::Struct(_))
                    || (fields.is_empty()
                        && matches!(
                            expected,
                            OpKind::Array(_) | OpKind::Slice(_) | OpKind::Map(_, _)
                        ))
                {
                    continue;
                }
            }
            let ik = self.kind_of_expr(item);
            if ik == OpKind::Unknown || ik == expected {
                continue;
            }
            if expected == OpKind::Float
                && ik == OpKind::Int
                && crate::const_overflow::const_int_value(item).is_some()
            {
                continue;
            }
            self.emit_e1002(
                item.span(),
                format!(
                    "cannot use `{}` (type `{}`) as `{}` value in array literal (HYB-E1002)",
                    item.render(),
                    ik.label(),
                    elem_ty.render()
                ),
                "every element of an array composite literal `[N]T{...}` / `[...]T{...}` must be assignable to the element type `T`",
            );
        }
    }

    /// Family D1 & Family E (#425): non-function call check + binding-shaped builtin arg checks.
    fn check_call_operands(&mut self, callee: &Expr, args: &[CallArg], span: Span) {
        let bare = unwrap_parens(callee);
        match bare {
            // `[]byte("ok")` / `[]rune(s)`: parser encodes slice type conversions as
            // `Call { callee: SliceLit { elem: Some(T), items: [] }, args: [x] }`.
            Expr::SliceLit {
                elem: Some(_),
                items,
                ..
            } if items.is_empty() && bare.as_array_lit().is_none() => {}
            Expr::Ident { name, .. } => {
                if let Some(k) = self.lookup_kind(name) {
                    match k {
                        // A local bound to a closure or fn-typed value carries
                        // its declared parameter count, so the call's arity is
                        // checkable (#577 C9).
                        OpKind::Fn(_, Some(want)) if want != args.len() => {
                            self.report_local_arity(name, want, args.len(), span)
                        }
                        OpKind::Fn(..) | OpKind::Unknown => {}
                        _ => self.emit_e1002(
                            span,
                            format!(
                                "cannot call `{}` (variable of type `{}`): `{}` is not a function (HYB-E1002)",
                                name,
                                k.label(),
                                k.label()
                            ),
                            "only functions, methods, closures, and type conversions can be called",
                        ),
                    }
                } else {
                    self.check_fn_type_arg_arity(name, name, span);
                    self.check_builtin_operand_kinds(name, args, span);
                    self.check_conversion_arity(name, args, span);
                    // #682 U5: `append`'s appended values must be assignable
                    // to the slice's element type.
                    if name == "append" {
                        self.check_append_elements(args);
                    }
                    self.check_user_fn_arity(name, args, span);
                    self.check_user_fn_arg_types(name, args);
                }
            }
            Expr::Field { base, name, .. } => {
                if let Expr::Ident { name: pkg, .. } = unwrap_parens(base) {
                    if pkg == "errors"
                        && name == "wrap"
                        && self.lookup_kind(pkg).is_none()
                        && args.len() != 2
                    {
                        self.emit_e1002(
                            span,
                            format!(
                                "`errors.wrap` takes 2 arguments `(err, message)`, got {} (HYB-E1002)",
                                args.len()
                            ),
                            "write `errors.wrap(err, \"context\")` (SPEC-LANGUAGE E2)",
                        );
                    }
                    if self.lookup_kind(pkg).is_none() {
                        let head = Checker::base_named_type_str(name);
                        let assoc_key = format!("{}.{}", pkg, head);
                        self.check_fn_type_arg_arity(&assoc_key, name, span);
                    }
                }
                let bk = self.kind_of_expr(base);
                // A method has no field entry, so resolve it before the field
                // lookup: `x.bump()` must reach the arity check even though
                // `bump` is not a struct field (#577 C9).
                // pr-678: normalize through `struct_base_name` so a generic
                // instantiation (`cache[int]`, `*lru_cache[k, v]`) resolves to
                // the base-name key `method_decls` is indexed by — the same
                // fold `base_named_type` applies for method receivers.
                let has_method = self
                    .struct_base_name(&bk)
                    .is_some_and(|sn| self.ck.method_decls.contains_key(&(sn, name.clone())));
                if let Some(fk) = self.struct_field_kind(&bk, name) {
                    if !has_method && !matches!(fk, OpKind::Unknown | OpKind::Fn(..)) {
                        self.emit_e1002(
                            span,
                            format!(
                                "cannot call `{}` (field of type `{}`): `{}` is not a function (HYB-E1002)",
                                callee.render(),
                                fk.label(),
                                fk.label()
                            ),
                            "only function-typed struct fields or declared methods can be called",
                        );
                    }
                    // `u.cb(1, 2)` where `cb fn(x int) int` — the field type
                    // states the count.
                    if !has_method {
                        if let OpKind::Fn(_, Some(want)) = fk {
                            if want != args.len() {
                                self.report_local_arity(&callee.render(), want, args.len(), span);
                            }
                        }
                    }
                } else if name == "pop" {
                    self.check_slice_pop_operands(base, &bk, args, span);
                } else if !has_method {
                    // #682 U2/U3: neither a declared method nor a field exists
                    // on the base. Go refuses the call with `<base>.<name>
                    // undefined (type <T> has no field or method <name>)`.
                    //
                    // Fail-open on anything unresolved: an `Unknown` base means
                    // the checker could not classify it, a `Struct(None)` has no
                    // name to look the method up under, and a partially
                    // unresolved shape (notably `Pointer(Unknown)` for a foreign
                    // generic like `*atomic.int64`) means the members are
                    // invisible to the kind model rather than absent. A foreign
                    // generic is the common case; `LOWERED_METHOD_INTRINSICS`
                    // covers the other one, a name `hy-emit` lowers to a
                    // prelude call rather than a Go method.
                    if is_fully_resolved(&bk) && !LOWERED_METHOD_INTRINSICS.contains(&name.as_str())
                    {
                        self.emit_e1002(
                            span,
                            format!(
                                "`{}` has no field or method `{}` (type `{}`) — Go refuses `{}.{} undefined` (HYB-E1002)",
                                base.render(),
                                name,
                                bk.label(),
                                base.render(),
                                name
                            ),
                            "declare the method on that type, or call something that exists",
                        );
                    }
                }
                if has_method {
                    self.check_method_arity(&bk, name, args, span);
                    self.check_method_arg_types(&bk, name, args);
                }
            }
            _ => {
                let ck = self.kind_of_expr(bare);
                if !matches!(ck, OpKind::Unknown | OpKind::Fn(..)) {
                    self.emit_e1002(
                        span,
                        format!(
                            "cannot call `{}` (type `{}`): `{}` is not a function (HYB-E1002)",
                            callee.render(),
                            ck.label(),
                            ck.label()
                        ),
                        "only functions, methods, closures, and type conversions can be called",
                    );
                }
            }
        }
    }

    fn check_fn_type_arg_arity(&mut self, fn_key: &str, raw_callee: &str, span: Span) {
        let Some(open) = raw_callee.find('[') else {
            return;
        };
        if !raw_callee.ends_with(']') {
            return;
        }
        let inner = &raw_callee[open + 1..raw_callee.len() - 1];
        for arg in split_top_level_type_args(inner) {
            self.check_named_type_instantiation(arg, span);
        }
        let base_key = Checker::base_named_type_str(fn_key);
        let Some(f) = self.ck.fns.get(base_key) else {
            return;
        };
        let expected = f.type_params.len();
        let got = split_top_level_type_args(inner).len();
        if expected == 0 {
            self.emit_e1002(
                span,
                format!(
                    "`{}` is not a generic function — cannot instantiate `{}` with type arguments (HYB-E1002)",
                    base_key, raw_callee
                ),
                "remove the `[...]` type arguments or declare type parameters on the function",
            );
        } else if got != expected {
            self.emit_e1002(
                span,
                format!(
                    "wrong number of type arguments for `{}` in `{}`: expected {}, found {} (HYB-E1002)",
                    base_key, raw_callee, expected, got
                ),
                "supply one type argument per declared type parameter on the function",
            );
        }
    }

    fn is_addressable_slice_pop_base(&self, e: &Expr) -> bool {
        match unwrap_parens(e) {
            Expr::Ident { .. } => true,
            Expr::Field { base, .. } => {
                matches!(self.kind_of_expr(base), OpKind::Pointer(_))
                    || self.is_addressable_slice_pop_base(base)
            }
            Expr::Index { base, .. } => match self.kind_of_expr(base) {
                OpKind::Slice(_) => true,
                OpKind::Array(_) => self.is_addressable_slice_pop_base(base),
                OpKind::Pointer(inner) if matches!(*inner, OpKind::Array(_)) => true,
                OpKind::Unknown => self.is_addressable_slice_pop_base(base),
                _ => false,
            },
            _ => false,
        }
    }

    /// Check method selection too: task callees and bound method values do
    /// not pass through the ordinary call-arity checker (#656).
    fn check_pointer_method_receiver(&mut self, base: &Expr, name: &str, span: Span) {
        let bk = self.kind_of_expr(base);
        let head = Checker::base_named_type_str(name);
        if let OpKind::Struct(Some(sn)) = &bk {
            let base_sn = Checker::base_named_type_str(sn);
            if let Some(m) = self
                .ck
                .method_decls
                .get(&(base_sn.to_string(), head.to_string()))
            {
                let ptr_recv = m.receiver.as_ref().is_some_and(|r| r.is_pointer);
                if ptr_recv && !self.is_addressable_value_receiver(base) {
                    self.emit_e1002(
                        span,
                        format!(
                            "cannot select pointer method `{}` on `{}` (non-addressable `{}` value) — Go refuses taking the address of `{}` (HYB-E1002)",
                            name,
                            base.render(),
                            sn,
                            base.render()
                        ),
                        "assign the value to a local variable first (`v := ...; v.method()`), store `*T` pointers, or declare a value receiver `(self T)`",
                    );
                }
            }
        }
    }

    /// Issue #656: can Go implicitly take `(&e)` to invoke a `*T` receiver
    /// method on a value-typed struct expression `e`?
    /// Addressable: local/param variables, pointer dereferences, slice
    /// indexing (`s[i]`), array indexing of an addressable array, and field
    /// selection on a pointer or addressable struct.
    /// Non-addressable: map index (`m[k]`), function/method call result
    /// (`make_t()`), and struct literal (`T{}`).
    fn is_addressable_value_receiver(&self, e: &Expr) -> bool {
        match unwrap_parens(e) {
            Expr::Ident { .. } => true,
            Expr::Index { base, .. } => match self.kind_of_expr(base) {
                OpKind::Map(_, _) | OpKind::Str => false,
                OpKind::Slice(_) | OpKind::Unknown => true,
                OpKind::Pointer(inner) => {
                    matches!(*inner, OpKind::Array(_) | OpKind::Slice(_) | OpKind::Unknown)
                }
                OpKind::Array(_) => self.is_addressable_value_receiver(base),
                _ => false,
            },
            Expr::Field { base, .. } => match self.kind_of_expr(base) {
                OpKind::Pointer(_) | OpKind::Unknown => true,
                _ => self.is_addressable_value_receiver(base),
            },
            _ => false,
        }
    }

    fn check_slice_pop_operands(&mut self, base: &Expr, bk: &OpKind, args: &[CallArg], span: Span) {
        match bk {
            OpKind::Slice(_) => {
                if !args.is_empty() {
                    self.emit_e2005(
                        span,
                        format!(
                            "slice `.pop()` takes 0 arguments, found {} in `{}.pop(...)` (HYB-E2005)",
                            args.len(),
                            base.render()
                        ),
                        "call `.pop()` with no arguments: `x := s.pop()`",
                    );
                }
                if !self.is_addressable_slice_pop_base(base) {
                    self.emit_e1002(
                        span,
                        format!(
                            "cannot call `.pop()` on non-addressable slice expression `{}` — `.pop()` mutates the slice header in place (HYB-E1002)",
                            base.render()
                        ),
                        "bind the slice to a local variable first (`s := ...; x := s.pop()`)",
                    );
                }
            }
            OpKind::Int
            | OpKind::Float
            | OpKind::Bool
            | OpKind::Str
            | OpKind::Array(_)
            | OpKind::Map(_, _)
            | OpKind::Chan(_) => {
                self.emit_e1002(
                    span,
                    format!(
                        "cannot call `.pop()` on `{}` (type `{}`) — `.pop()` is only defined on slices `[]T` (HYB-E1002)",
                        base.render(),
                        bk.label()
                    ),
                    "`.pop()` is a slice operation (`[]T`); fixed-size arrays, maps, channels, and primitives do not support `.pop()`",
                );
            }
            _ => {}
        }
    }
    /// Family E1–E5 (#425): extend `HYB-E2005` (`BUILTIN-ARGS`) from literals
    /// (`builtins.rs`) to resolved binding shapes.
    fn check_builtin_operand_kinds(&mut self, name: &str, args: &[CallArg], span: Span) {
        match name {
            "len" => {
                if args.len() != 1 {
                    return; // arity handled by builtins.rs
                }
                let op = &args[0].value;
                if builtins_already_flags_len_cap(op, false) {
                    return;
                }
                let ak = self.kind_of_expr(op);
                let ok = matches!(
                    ak,
                    OpKind::Unknown
                        | OpKind::Str
                        | OpKind::Slice(_)
                        | OpKind::Array(_)
                        | OpKind::Map(_, _)
                        | OpKind::Chan(_)
                ) || matches!(
                    &ak,
                    OpKind::Pointer(inner) if matches!(**inner, OpKind::Unknown | OpKind::Array(_))
                );
                if !ok {
                    self.emit_e2005(
                        span,
                        format!(
                            "invalid argument `{}` (type `{}`) for built-in `len` (HYB-E2005)",
                            op.render(),
                            ak.label()
                        ),
                        "`len` accepts a string, slice, array, map, or channel",
                    );
                }
            }
            "cap" => {
                if args.len() != 1 {
                    return; // arity handled by builtins.rs
                }
                let op = &args[0].value;
                if builtins_already_flags_len_cap(op, true) {
                    return;
                }
                let ak = self.kind_of_expr(op);
                let ok = matches!(
                    ak,
                    OpKind::Unknown | OpKind::Slice(_) | OpKind::Array(_) | OpKind::Chan(_)
                ) || matches!(
                    &ak,
                    OpKind::Pointer(inner) if matches!(**inner, OpKind::Unknown | OpKind::Array(_))
                );
                if !ok {
                    self.emit_e2005(
                        span,
                        format!(
                            "invalid argument `{}` (type `{}`) for built-in `cap` (HYB-E2005)",
                            op.render(),
                            ak.label()
                        ),
                        "`cap` accepts a slice, array, or channel — maps and strings have no capacity",
                    );
                }
            }
            "close" => {
                if args.len() != 1 {
                    return; // arity handled by builtins.rs
                }
                let op = &args[0].value;
                if builtins_already_flags_close(op) {
                    return;
                }
                let ak = self.kind_of_expr(op);
                if !matches!(ak, OpKind::Unknown | OpKind::Chan(_)) {
                    self.emit_e2005(
                        span,
                        format!(
                            "cannot close non-channel `{}` (type `{}`) (HYB-E2005)",
                            op.render(),
                            ak.label()
                        ),
                        "`close` requires a bidirectional or send-only channel (`chan T` or `chan<- T`)",
                    );
                }
            }
            "delete" => {
                if args.len() != 2 {
                    return; // arity handled by builtins.rs
                }
                let map_arg = &args[0].value;
                let key_arg = &args[1].value;
                let mk = self.kind_of_expr(map_arg);
                match &mk {
                    OpKind::Unknown => {}
                    OpKind::Map(k_expected, _) => {
                        let kk = self.kind_of_expr(key_arg);
                        if **k_expected != OpKind::Unknown
                            && kk != OpKind::Unknown
                            && kk != **k_expected
                        {
                            self.emit_e2005(
                                span,
                                format!(
                                    "cannot use `{}` (type `{}`) as key type `{}` in `delete` (HYB-E2005)",
                                    key_arg.render(),
                                    kk.label(),
                                    k_expected.label()
                                ),
                                "the second argument to `delete(m, k)` must match the map's key type",
                            );
                        }
                    }
                    _ => {
                        self.emit_e2005(
                            span,
                            format!(
                                "invalid argument `{}` (type `{}`) is not a map in `delete` (HYB-E2005)",
                                map_arg.render(),
                                mk.label()
                            ),
                            "`delete(m, k)` requires a map as its first argument",
                        );
                    }
                }
            }
            "append" => {
                if args.is_empty() {
                    self.emit_e2005(
                        span,
                        "`append` requires at least 1 argument (slice[, elems...]) (HYB-E2005)"
                            .to_string(),
                        "pass a slice as the first argument to `append(xs, elem)`",
                    );
                    return;
                }
                let slice_arg = &args[0].value;
                let sk = self.kind_of_expr(slice_arg);
                if !matches!(sk, OpKind::Unknown | OpKind::Slice(_)) {
                    self.emit_e2005(
                        span,
                        format!(
                            "invalid `append`: first argument `{}` (type `{}`) must be a slice (HYB-E2005)",
                            slice_arg.render(),
                            sk.label()
                        ),
                        "the first argument to `append(xs, ...)` must be a slice (`[]T`)",
                    );
                }
            }
            _ => {}
        }
    }

    // -- results -------------------------------------------------------------

    /// Form 2: a declared Go import whose root is never mentioned anywhere
    /// in the file is carried into the emitted header and never builds.
    /// `rust/` imports are skipped — the emitter never writes them.
    fn check_imports(&mut self, file: &File) {
        for d in &file.decls {
            let Decl::Import { path, span } = d else {
                continue;
            };
            if path.starts_with("rust/") || !crate::foreign::is_curated_import(path) {
                continue;
            }
            let root = path.rsplit('/').next().unwrap_or(path.as_str());
            if !self.roots.contains(root) {
                self.diags.push(
                    Diagnostic::error(
                        "HYB-E1002",
                        self.path.clone(),
                        *span,
                        format!(
                            "import `{}` is never used — the emitted Go carries it into the header and Go refuses `imported and not used` (HYB-E1002)",
                            root
                        ),
                    )
                    .with_notes(vec![
                        "drop the import, or use it — any mention of the package root counts"
                            .to_string(),
                    ]),
                );
            }
        }
    }

    /// All fn-local declarations have the same Go unused-binding rule.
    fn finish(&mut self) {
        for d in &self.defines {
            if !d.used && (!d.elidable || d.written) {
                let emitted = match d.kind {
                    LocalKind::Define | LocalKind::Let => format!("{} := ...", d.name),
                    LocalKind::Var | LocalKind::Const => format!("var {}", d.name),
                };
                self.diags.push(
                    Diagnostic::error(
                        "HYB-E1002",
                        self.path.clone(),
                        d.span,
                        format!(
                            "`{}` is declared and never used — Go refuses to build the emitted `{}` (declared and not used) (HYB-E1002)",
                            d.name, emitted
                        ),
                    )
                    .with_notes(vec![format!(
                        "a read anywhere in this function counts, including inside a closure; `_ = {}` also counts as a use",
                        d.name
                    )]),
                );
            }
        }
    }
}

fn span_expr_label(lhs: &Expr, op: &str, rhs: &Expr) -> String {
    format!("{} {} {}", lhs.render(), op, rhs.render())
}

fn builtins_already_flags_close(e: &Expr) -> bool {
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

fn builtins_already_flags_len_cap(e: &Expr, is_cap: bool) -> bool {
    let bare = unwrap_parens(e);
    let num_bool_char = match bare {
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
    };
    if num_bool_char {
        return true;
    }
    is_cap
        && matches!(
            bare,
            Expr::Lit {
                kind: LitKind::Str(_) | LitKind::RawStr(_),
                ..
            }
        )
}

impl<'a> Checker<'a> {
    /// Issue #722: validate generic type parameter lists on struct and function
    /// declarations (duplicate parameter names, unsupported constraints outside
    /// SPEC G3 `any`/`comparable`, and explicit `any` constraint used as a map
    /// key or set element).
    pub(crate) fn check_type_params<'t>(
        &mut self,
        decl_name: &str,
        decl_kind: &str,
        type_params: &[hy_syntax::TypeParam],
        sig_types: impl Iterator<Item = &'t TypeExpr>,
        span: Span,
    ) {
        if type_params.is_empty() {
            return;
        }
        let mut seen = HashSet::new();
        for p in type_params {
            if !seen.insert(p.name.as_str()) {
                self.diags.push(
                    Diagnostic::error(
                        "HYB-E1003",
                        self.path.clone(),
                        span,
                        format!(
                            "type parameter `{}` redeclared in {} `{}` (HYB-E1003)",
                            p.name, decl_kind, decl_name
                        ),
                    )
                    .with_notes(vec![
                        format!(
                            "why: Go rejects duplicate type parameter names (`{} redeclared in this block`)",
                            p.name
                        ),
                        "fix: give each type parameter a distinct identifier".to_string(),
                    ]),
                );
            }
            if let Some(c) = &p.constraint {
                if c != "any" && c != "comparable" {
                    self.diags.push(
                        Diagnostic::error(
                            "HYB-E1002",
                            self.path.clone(),
                            span,
                            format!(
                                "unknown or unsupported type constraint `{}` on type parameter `{}` in {} `{}` (HYB-E1002)",
                                c, p.name, decl_kind, decl_name
                            ),
                        )
                        .with_notes(vec![
                            "SPEC G3 supports only `any` and `comparable` type constraints".to_string(),
                        ]),
                    );
                }
            }
        }
        let param_names: HashSet<&str> = type_params.iter().map(|p| p.name.as_str()).collect();
        let mut comparable_needed = HashSet::new();
        for ty in sig_types {
            collect_comparable_type_params(ty, &param_names, &mut comparable_needed);
        }
        for p in type_params {
            if p.constraint.as_deref() == Some("any")
                && comparable_needed.contains(p.name.as_str())
            {
                self.diags.push(
                    Diagnostic::error(
                        "HYB-E1002",
                        self.path.clone(),
                        span,
                        format!(
                            "type parameter `{}` has explicit constraint `any` in {} `{}`, but is used as a map key or set element which requires `comparable` (HYB-E1002)",
                            p.name, decl_kind, decl_name
                        ),
                    )
                    .with_notes(vec![
                        format!(
                            "change `[{} any]` to `[{} comparable]` (or omit `any` so the compiler infers `comparable`)",
                            p.name, p.name
                        ),
                    ]),
                );
            }
        }
    }
}

fn collect_comparable_type_params<'a>(
    ty: &'a TypeExpr,
    param_names: &HashSet<&'a str>,
    out: &mut HashSet<String>,
) {
    match ty {
        TypeExpr::Map { key, value, .. } => {
            if let TypeExpr::Named { name, .. } = key.as_ref() {
                if param_names.contains(name.as_str()) {
                    out.insert(name.clone());
                }
            }
            collect_comparable_type_params(key, param_names, out);
            collect_comparable_type_params(value, param_names, out);
        }
        TypeExpr::Named { name, .. } => {
            if let Some(inner) = name.strip_prefix("set[").and_then(|s| s.strip_suffix(']')) {
                let elem = inner.trim();
                if param_names.contains(elem) {
                    out.insert(elem.to_string());
                }
            }
        }
        TypeExpr::Pointer { to, .. } => collect_comparable_type_params(to, param_names, out),
        TypeExpr::Slice { elem, .. }
        | TypeExpr::Array { elem, .. }
        | TypeExpr::Chan { elem, .. }
        | TypeExpr::Option { elem, .. } => {
            collect_comparable_type_params(elem, param_names, out);
        }
        TypeExpr::Fn {
            params, results, ..
        } => {
            for p in params {
                collect_comparable_type_params(&p.ty, param_names, out);
            }
            for r in results {
                collect_comparable_type_params(r, param_names, out);
            }
        }
    }
}

fn split_top_level_type_args(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (i, ch) in s.char_indices() {
        match ch {
            '[' | '(' => depth += 1,
            ']' | ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                out.push(s[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    let tail = s[start..].trim();
    if !tail.is_empty() {
        out.push(tail);
    }
    out
}

#[cfg(test)]
mod operand_kind_tests {
    use crate::{check_file, Verdict};
    use hy_diag::Severity;
    use hy_syntax::parse_file;

    fn check(src: &str) -> crate::CheckResult {
        let ast = parse_file("test.hy", src).expect("test snippet must parse");
        check_file(&ast)
    }

    fn error_codes(r: &crate::CheckResult) -> Vec<String> {
        r.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| d.code.clone())
            .collect()
    }

    #[test]
    fn rejects_non_boolean_conditions_a1_a2() {
        for src in [
            "fn main() {\n    x := 5\n    if x { fmt.println(1) }\n}\n",
            "fn main() {\n    x := 5\n    for x { x = 0 }\n}\n",
        ] {
            let r = check(src);
            assert_eq!(r.verdict, Verdict::Reject, "expected reject for:\n{src}");
            assert!(
                error_codes(&r).contains(&"HYB-E1002".to_string()),
                "expected HYB-E1002, got {:?} for:\n{src}",
                error_codes(&r)
            );
        }
    }

    #[test]
    fn rejects_operator_operand_mismatches_b1_to_b6() {
        for src in [
            "fn main() {\n    x := 5\n    fmt.println(!x)\n}\n",
            "fn main() {\n    s := \"a\"\n    fmt.println(-s)\n}\n",
            "fn main() {\n    x := 5\n    s := \"a\"\n    fmt.println(x + s)\n}\n",
            "fn main() {\n    x := 5\n    fmt.println(x && true)\n}\n",
            "fn main() {\n    b := true\n    fmt.println(b < false)\n}\n",
            "fn main() {\n    s := \"a\"\n    s++\n    fmt.println(s)\n}\n",
        ] {
            let r = check(src);
            assert_eq!(r.verdict, Verdict::Reject, "expected reject for:\n{src}");
            assert!(
                error_codes(&r).contains(&"HYB-E1002".to_string()),
                "expected HYB-E1002, got {:?} for:\n{src}",
                error_codes(&r)
            );
        }
    }

    #[test]
    fn rejects_non_indexable_and_non_callable_c1_c2_d1() {
        for src in [
            "fn main() {\n    x := 5\n    fmt.println(x[0])\n}\n",
            "fn main() {\n    x := 5\n    fmt.println(x[0:1])\n}\n",
            "fn main() {\n    x := 5\n    fmt.println(x())\n}\n",
        ] {
            let r = check(src);
            assert_eq!(r.verdict, Verdict::Reject, "expected reject for:\n{src}");
            assert!(
                error_codes(&r).contains(&"HYB-E1002".to_string()),
                "expected HYB-E1002, got {:?} for:\n{src}",
                error_codes(&r)
            );
        }
    }

    #[test]
    fn rejects_binding_shaped_builtin_misuse_e1_to_e5() {
        for src in [
            "fn main() {\n    x := 5\n    fmt.println(len(x))\n}\n",
            "fn main() {\n    x := 5\n    close(x)\n}\n",
            "fn main() {\n    m := map[string]int{}\n    fmt.println(cap(m))\n}\n",
            "fn main() {\n    xs := []int{1}\n    delete(xs, 0)\n    fmt.println(xs)\n}\n",
            "fn main() {\n    x := 5\n    x = append(x, 1)\n    fmt.println(x)\n}\n",
        ] {
            let r = check(src);
            assert_eq!(r.verdict, Verdict::Reject, "expected reject for:\n{src}");
            assert!(
                error_codes(&r).contains(&"HYB-E2005".to_string()),
                "expected HYB-E2005, got {:?} for:\n{src}",
                error_codes(&r)
            );
        }
    }

    #[test]
    fn accepts_valid_operand_and_builtin_controls() {
        let src = "\
fn main() {
    ok := true
    if ok {
        fmt.println(!ok)
    }
    n := 5
    fmt.println(-n)
    fmt.println(n + 2)
    s := \"hello\"
    fmt.println(s + \" world\")
    fmt.println(s[0:2])
    fmt.println(len(s))
    xs := []int{1, 2}
    xs = append(xs, 3)
    fmt.println(xs[0])
    fmt.println(cap(xs))
    m := map[string]int{\"k\": 1}
    delete(m, \"k\")
    fmt.println(len(m))
    ch := make(chan int, 1)
    close(ch)
    f := fn() int { return 7 }
    fmt.println(f())
}
";
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "unexpected diagnostics: {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn rejects_constant_string_index_and_slice_bounds_708() {
        for src in [
            "const s = \"ab\"\nfn main() {\n    fmt.println(s[5])\n}\n",
            "fn main() {\n    fmt.println(\"hello\"[7])\n}\n",
            "const s = \"ab\"\nfn main() {\n    fmt.println(s[0:5])\n}\n",
            "fn main() {\n    fmt.println(\"hello\"[1:7])\n}\n",
            "fn main() {\n    const s = \"ab\" + \"cd\"\n    fmt.println(s[4])\n}\n",
            "fn main() {\n    fmt.println(\"hello\"[3:2])\n}\n",
            "fn main() {\n    fmt.println(\"hello\"[-1])\n}\n",
        ] {
            let r = check(src);
            assert_eq!(r.verdict, Verdict::Reject, "expected reject for:\n{src}");
            assert!(
                error_codes(&r).contains(&"HYB-E1002".to_string()),
                "expected HYB-E1002, got {:?} for:\n{src}",
                error_codes(&r)
            );
        }
    }

    #[test]
    fn accepts_constant_string_bounds_controls_708() {
        let src = "\
const s = \"ab\"
fn main() {
    fmt.println(\"hello\"[4])
    fmt.println(s[1])
    fmt.println(\"hello\"[0:5])
    fmt.println(\"hello\"[2:2])
    dyn_s := \"ab\"
    fmt.println(dyn_s[5])
}
";
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "unexpected diagnostics: {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn rejects_malformed_generic_structs_and_instantiations_722() {
        // Uninstantiated generic struct, wrong type-arg arity, type args on
        // non-generic struct/fn, wrong field value type after substitution,
        // non-comparable `any` used as map key, and unsupported constraint.
        for src in [
            "struct box[t] { v t }\nfn main() {\n    var b box\n    fmt.println(b.v)\n}\n",
            "struct box[t] { v t }\nfn main() {\n    b := box{ v: 1 }\n    fmt.println(b.v)\n}\n",
            "struct box[t] { v t }\nfn main() {\n    b := box[int, string]{ v: 1 }\n    fmt.println(b.v)\n}\n",
            "struct plain { v int }\nfn main() {\n    p := plain[int]{ v: 1 }\n    fmt.println(p.v)\n}\n",
            "struct box[t] { v t }\nfn main() {\n    b := box[int]{ v: \"wrong\" }\n    fmt.println(b.v)\n}\n",
            "struct box[t] { v t }\nfn main() {\n    b := box[int]{ v: 3 }\n    fmt.println(b.v + \"bad\")\n}\n",
            "struct bad[k any] { m map[k]int }\nfn main() {}\n",
            "struct bad[t bogus] { v t }\nfn main() {}\n",
            "fn id[t](x t) t { return x }\nfn main() {\n    fmt.println(id[int, string](1))\n}\n",
            "fn add(a int, b int) int { return a + b }\nfn main() {\n    fmt.println(add[int](1, 2))\n}\n",
        ] {
            let r = check(src);
            assert_eq!(r.verdict, Verdict::Reject, "expected reject for:\n{src}");
            assert!(
                error_codes(&r).contains(&"HYB-E1002".to_string()),
                "expected HYB-E1002, got {:?} for:\n{src}",
                error_codes(&r)
            );
        }

        // Duplicate type parameter names -> HYB-E1003.
        for src in [
            "struct box[t, t] { v t }\nfn main() {}\n",
            "fn id[t, t](x t) t { return x }\nfn main() {}\n",
        ] {
            let r = check(src);
            assert_eq!(r.verdict, Verdict::Reject, "expected reject for:\n{src}");
            assert!(
                error_codes(&r).contains(&"HYB-E1003".to_string()),
                "expected HYB-E1003, got {:?} for:\n{src}",
                error_codes(&r)
            );
        }
    }

    #[test]
    fn accepts_valid_generic_struct_and_fn_722() {
        let src = "\
struct box[t] {
    v t
}

struct pair[k, v] {
    key k
    val v
}

fn id[t](x t) t {
    return x
}

fn main() {
    b := box[int]{ v: 3 }
    p := pair[string, int]{ key: \"ok\", val: b.v + 1 }
    fmt.println(p.key, id[int](p.val))
}
";
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "unexpected diagnostics: {:?}",
            r.diagnostics
        );
    }
}
