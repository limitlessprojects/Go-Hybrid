//! hy-emit — the MVP-0 Go emitter (the walking skeleton's last leg).
//!
//! Pipeline position: `hy-syntax` (AST) → `hy-check` (Tier-0 verdict) →
//! **hy-emit** (Go source) → `go build` → executable.
//!
//! Scope (MVP-0): the surface exercised by examples/01-hello, 02-cli-cat,
//! 03-worker-pool — plus the shared forms the corpus uses (for-forms, send/
//! receive, task→go, move-marker drop, defer, interpolation→Sprintf).
//! Anything outside that surface returns a CLEAN unsupported error — never
//! a wrong emission (same discipline as the parser's AST contract).
//!
//! Design notes:
//!   * TWO-PASS file assembly — decls render into a body buffer while used
//!     packages collect into a BTreeSet; the header (package + imports) is
//!     assembled LAST. A struct-first file therefore can never emit `type`
//!     before `import` (Go rejects that; this was MVP-0 bug #1).
//!   * `make(chan job, 16)` — make's first argument renders as a BARE TYPE,
//!     never a value literal (the parser encodes type-as-value as an empty
//!     `StructLit{ty: Some(t)}`; rendering it as `job{}` inside make() was
//!     MVP-0 bug #2).
//!   * `?` desugaring follows SPEC-LANGUAGE.md §6 (E3) — temp error var +
//!     early return; and the main-level exit contract (E6): a wrapper runs
//!     defers (LIFO), prints `"<prog>: {err}"` to stderr, exits 1.
//!   * Foreign-name normalization (M4): Hybrid's snake_case members map to
//!     Go's exported names at emission (`read_all` → `ReadAll`), with a
//!     special-case table for the shapes Go spells differently
//!     (`os.args()` → `os.Args`, `fmt.eprintln` → `fmt.Fprintln(os.Stderr, …)`).

use std::collections::{BTreeMap, BTreeSet};

use hy_syntax::*;

mod constconv;
mod rangebind;
mod shapes;
mod zeroinit;
use shapes::{
    import_path_of, lookup_conversion, lookup_rust_crate, lookup_unbound_crate, Conversion,
};

// ---------------------------------------------------------------------------
// Hybrid prelude (auto-generated Go, emitted only when the program uses it)
// ---------------------------------------------------------------------------

/// `option[T]` — a payload + ok-flag struct; `none` is the zero value.
/// `option`, `set`, and `hy_pop` are RESERVED stdlib names (Decision Log).
const OPTION_PRELUDE: &str = concat!(
    "// ---- Hybrid prelude: option (auto-generated; do not edit) ----\n",
    "type option[v any] struct {\n",
    "\tval v\n",
    "\tok  bool\n",
    "}\n",
    "\n",
    "func some[v any](val v) option[v] {\n",
    "\treturn option[v]{val: val, ok: true}\n",
    "}\n",
    "\n",
    "func (o option[v]) unwrap() v {\n",
    "\tif !o.ok {\n",
    "\t\tpanic(\"HY-PANIC/unwrap-none\")\n",
    "\t}\n",
    "\treturn o.val\n",
    "}\n",
    "\n",
    "func (o option[v]) or(defaultVal v) v {\n",
    "\tif !o.ok {\n",
    "\t\treturn defaultVal\n",
    "\t}\n",
    "\treturn o.val\n",
    "}\n",
    "\n",
    "func (o option[v]) is_some() bool {\n",
    "\treturn o.ok\n",
    "}\n",
    "\n",
    "func (o option[v]) is_none() bool {\n",
    "\treturn !o.ok\n",
    "}\n",
);

const SET_PRELUDE: &str = concat!(
    "// ---- Hybrid prelude: set (auto-generated; do not edit) ----\n",
    "type set[t comparable] struct {\n",
    "\tm map[t]struct{}\n",
    "}\n",
    "\n",
    "func (s *set[t]) insert(v t) bool {\n",
    "\tif s.m == nil {\n",
    "\t\ts.m = make(map[t]struct{})\n",
    "\t}\n",
    "\t_, existed := s.m[v]\n",
    "\ts.m[v] = struct{}{}\n",
    "\treturn !existed\n",
    "}\n",
    "\n",
    "func (s *set[t]) contains(v t) bool {\n",
    "\t_, ok := s.m[v]\n",
    "\treturn ok\n",
    "}\n",
    "\n",
    "func (s *set[t]) remove(v t) {\n",
    "\tdelete(s.m, v)\n",
    "}\n",
    "\n",
    "func (s set[t]) len() int {\n",
    "\treturn len(s.m)\n",
    "}\n",
);

const POP_PRELUDE: &str = concat!(
    "// ---- Hybrid prelude: slice pop (auto-generated; do not edit) ----\n",
    "func hy_pop[t any](s *[]t) t {\n",
    "\tn := len(*s)\n",
    "\tv := (*s)[n-1]\n",
    "\t*s = (*s)[:n-1]\n",
    "\treturn v\n",
    "}\n",
);

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// A clean "surface not implemented by the MVP-0 emitter" error.
/// Never a wrong emission — the caller prints it and stops.
#[derive(Debug, Clone)]
pub struct EmitError {
    pub message: String,
}

impl EmitError {
    fn unsupported(what: impl Into<String>) -> Self {
        EmitError {
            message: format!("emitter (MVP-0): not yet supported: {}", what.into()),
        }
    }

    pub fn export_name_collision(name1: &str, name2: &str, go_name: &str) -> Self {
        EmitError {
            message: format!(
                "exported function name collision: `{}` and `{}` both normalize to `{}` in package mode",
                name1, name2, go_name
            ),
        }
    }

    /// The honest unshipped-crate error (the adoption-audit follow-up): the
    /// checker's foreign table knows the crate's type shapes, but the
    /// runtime ships no binding for it — the program cannot build or run.
    /// Deliberately NOT "not yet supported": the crate is known, the
    /// decision was made, the diagnostic should say so.
    pub fn unbound_crate(crate_name: &str) -> Self {
        EmitError {
            message: format!(
                "the runtime does not ship a `{crate_name}` binding — the checker's foreign table knows its type shapes (that is what drives the A7 consume-mode fixture), so `hyb check` passes, but no Go binding package is vendored and this program cannot build or run; the external audit priced shipping the crate at 250-400 lines and declined. Remove the use, or vendor a binding (runtime/go/{crate_name}binding + the RUST_CRATES descriptor) and drop `{crate_name}` from UNBOUND_RUST_CRATES in the same commit"
            ),
        }
    }
}

/// Emit one parsed Hybrid file as compilable Go source (`package main`).
pub fn emit_go(file: &File) -> Result<String, EmitError> {
    let mut em = Emitter::new();
    em.emit_file(file)
}

/// Emit as an importable PACKAGE (example 09's adoption story):
/// `package <name>`, `pub fn` exports via M4's export mapping, and the
/// binding packages import under `binding_prefix` (the consumer module's
/// path to the package dir). E3 propagates through error-result fns.
pub fn emit_go_package(
    file: &File,
    pkg_name: &str,
    binding_prefix: &str,
) -> Result<String, EmitError> {
    // Detect public function export name collisions in package mode (issue #1421)
    let mut exported_fns: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for decl in &file.decls {
        if let hy_syntax::Decl::Fn(f) = decl {
            if f.is_pub && f.receiver.is_none() && !f.name.contains('.') {
                let go_name = to_go_name(&f.name);
                if let Some(prev) = exported_fns.get(&go_name) {
                    return Err(EmitError::export_name_collision(prev, &f.name, &go_name));
                }
                exported_fns.insert(go_name, f.name.clone());
            }
        }
    }

    let mut em = Emitter::new();
    em.package_name = pkg_name.to_string();
    em.binding_prefix = binding_prefix.to_string();
    em.export_pub = true;
    em.emit_file(file)
}

/// Which rust-binding packages does the emitted Go call? (`hyb build`
/// assembles those into the build workspace.) Scans for the binding roots'
/// call prefix — the emitter only emits `<root>.` for KNOWN crates, so the
/// scan is exact, never heuristic.
pub fn used_binding_roots(go_src: &str) -> Vec<&'static str> {
    shapes::RUST_CRATES
        .iter()
        .filter(|c| go_src.contains(&format!("{}.", c.binding_root)))
        .map(|c| c.binding_root)
        .collect()
}

// ---------------------------------------------------------------------------
// Binding hints (the tiny type table the MVP-0 surface needs)
// ---------------------------------------------------------------------------

/// What the emitter knows about a local binding. This is deliberately NOT
/// a type checker — just enough to (a) normalize foreign members, (b) route
/// `fmt.print` on a byte buffer to `os.Stdout.Write` (byte dumps read as
/// `[104 105]`, not bytes), and (c) leave our own struct fields alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingKind {
    /// Bound from a foreign (package-qualified) call — members normalize
    /// (`f := os.open(p)` → `f.Close()`).
    Foreign,
    /// Bound with a KNOWN descriptor type (shapes::SHAPES key) — members
    /// resolve through the shape table field-vs-method (`r.url.query.get`
    /// → `r.URL.Query().Get`, Batch E / example 06).
    ForeignTy(&'static str),
    /// Bound from `io.read_all` — a byte buffer: `fmt.print(b)` becomes
    /// `os.Stdout.Write(b)`.
    Bytes,
    /// Bound to a channel (param type or `make(chan …)`) — single-var
    /// `for v in ch` ranges the VALUE directly; slices/maps need `_, v`.
    Chan,
    /// Bound to a set (`set[T]`) — `len(s)` / `delete(s, v)` / `for v in s`.
    Set,
    /// Our own struct / plain value — members emit verbatim.
    Plain,
}

// ---------------------------------------------------------------------------
// Go builtin types + member normalization (M4)
// ---------------------------------------------------------------------------

/// Go's predeclared type names — never CamelCase-normalized.
#[allow(dead_code)]
const GO_BUILTIN_TYPES: &[&str] = &[
    "bool",
    "byte",
    "rune",
    "string",
    "error",
    "any",
    "int",
    "int8",
    "int16",
    "int32",
    "int64",
    "uint",
    "uint8",
    "uint16",
    "uint32",
    "uint64",
    "uintptr",
    "float32",
    "float64",
    "complex64",
    "complex128",
];

/// Standard Go initialisms (SPEC M4: `io.eof` → `io.EOF`,
/// `http.status_ok` → `http.StatusOK`, `time.rfc3339` → `time.RFC3339`,
/// `strings.to_valid_utf8` → `strings.ToValidUTF8`).
fn go_initialism(seg: &str) -> Option<&'static str> {
    Some(match seg {
        "acl" => "ACL",
        "api" => "API",
        "ascii" => "ASCII",
        "cpu" => "CPU",
        "crlf" => "CRLF",
        "css" => "CSS",
        "dns" => "DNS",
        "eof" => "EOF",
        "guid" => "GUID",
        "html" => "HTML",
        "http" => "HTTP",
        "https" => "HTTPS",
        "id" => "ID",
        "ip" => "IP",
        "ipv4" => "IPv4",
        "ipv6" => "IPv6",
        "json" => "JSON",
        "lhs" => "LHS",
        "ok" => "OK",
        "qps" => "QPS",
        "ram" => "RAM",
        "rfc822" => "RFC822",
        "rfc822z" => "RFC822Z",
        "rfc850" => "RFC850",
        "rfc1123" => "RFC1123",
        "rfc1123z" => "RFC1123Z",
        "rfc3339" => "RFC3339",
        "rhs" => "RHS",
        "rpc" => "RPC",
        "sha1" => "SHA1",
        "sha256" => "SHA256",
        "sha384" => "SHA384",
        "sha512" => "SHA512",
        "sla" => "SLA",
        "smtp" => "SMTP",
        "sql" => "SQL",
        "ssh" => "SSH",
        "tcp" => "TCP",
        "tls" => "TLS",
        "ttl" => "TTL",
        "udp" => "UDP",
        "ui" => "UI",
        "uid" => "UID",
        "uri" => "URI",
        "url" => "URL",
        "utf8" => "UTF8",
        "utf16" => "UTF16",
        "utf32" => "UTF32",
        "uuid" => "UUID",
        "vm" => "VM",
        "xml" => "XML",
        "xmpp" => "XMPP",
        "xsrf" => "XSRF",
        "xss" => "XSS",
        _ => return None,
    })
}

/// snake_case → Go exported name (`read_all` → `ReadAll`, `args` → `Args`,
/// `eof` → `EOF`, `status_ok` → `StatusOK`).
pub(crate) fn to_go_name(name: &str) -> String {
    name.split('_')
        .filter(|p| !p.is_empty())
        .map(|p| {
            if let Some(init) = go_initialism(p) {
                return init.to_string();
            }
            let mut cs = p.chars();
            match cs.next() {
                Some(c) => c.to_uppercase().collect::<String>() + cs.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// The base NAMED type of a TypeExpr, if any (`*atomic.int64` →
/// "atomic.int64", `env` → "env", `map[string]int` → None). Batch I's
/// receiver tables key on this.
pub(crate) fn base_ty_name(t: &TypeExpr) -> Option<String> {
    match t {
        TypeExpr::Named { name, .. } => Some(name.clone()),
        TypeExpr::Pointer { to, .. } => base_ty_name(to),
        _ => None,
    }
}

/// Is this ident a foreign package root we recognize? (Import collection is
/// usage-driven: any dotted name's first segment gets an import line.)
pub(crate) fn is_package_root(name: &str) -> bool {
    // Everything the curated surface imports today. Unknown roots still work —
    // they emit verbatim and get an import — this list only guards the
    // special-case table below.
    matches!(
        name,
        "fmt"
            | "os"
            | "io"
            | "time"
            | "sort"
            | "strings"
            | "strconv"
            | "errors"
            | "sync"
            | "bufio"
            | "http"
            | "httptest"
            | "json"
            | "atomic"
            | "filepath"
            | "path"
            | "hex"
            | "sha3"
            | "serde_json"
            | "regex"
    )
}

/// Members Go spells differently than the naive M4 mapping.
/// Value: `None` = drop the call parens (it is a variable), `Some(text)` =
/// emit this expression text instead of `pkg.member`.
pub(crate) fn special_member(pkg: &str, name: &str) -> Option<MemberSpecial> {
    match (pkg, name) {
        ("os", "args") => Some(MemberSpecial::Var("os.Args")),
        ("os", "stdout") => Some(MemberSpecial::Var("os.Stdout")),
        ("os", "stderr") => Some(MemberSpecial::Var("os.Stderr")),
        ("os", "stdin") => Some(MemberSpecial::Var("os.Stdin")),
        // fmt's stderr family carries the writer as the first argument.
        ("fmt", "eprintln") => Some(MemberSpecial::StderrCall("Fprintln")),
        ("fmt", "eprint") => Some(MemberSpecial::StderrCall("Fprint")),
        ("fmt", "eprintf") => Some(MemberSpecial::StderrCall("Fprintf")),
        // Duration constructors — conversions, not funcs.
        ("time", "milliseconds") => Some(MemberSpecial::Duration("Millisecond")),
        ("time", "seconds") => Some(MemberSpecial::Duration("Second")),
        ("time", "minutes") => Some(MemberSpecial::Duration("Minute")),
        ("time", "hours") => Some(MemberSpecial::Duration("Hour")),
        // hex.encode → Go's EncodeToString (example 10; M5 allowlist growth).
        ("hex", "encode") => Some(MemberSpecial::Renamed("EncodeToString")),
        // errors.wrap(err, "ctx") → fmt.Errorf("%s: %w", "ctx", err) (E2 wrap-on-demand)
        ("errors", "wrap") => Some(MemberSpecial::ErrorsWrap),
        _ => None,
    }
}

pub(crate) enum MemberSpecial {
    /// A package-level VARIABLE — emit verbatim text, no call parens.
    Var(&'static str),
    /// `fmt.eprintln(args…)` → `fmt.Fprintln(os.Stderr, args…)`.
    StderrCall(&'static str),
    /// `time.milliseconds(n)` → `time.Duration(n) * time.Millisecond`.
    Duration(&'static str),
    /// A member Go spells differently: `hex.encode(b)` → `hex.EncodeToString(b)`.
    Renamed(&'static str),
    /// `errors.wrap(err, msg)` → `fmt.Errorf("%s: %w", msg, err)` (E2 / Spike 5)
    ErrorsWrap,
}

// ---------------------------------------------------------------------------
// The emitter
// ---------------------------------------------------------------------------

/// One fn's result shape row: (n_results, ends_in_error, first result's
/// binding kind, per-param pointer-base types). See Emitter::fn_shapes.
type FnShape = (usize, bool, Option<BindingKind>, Vec<Option<String>>);

pub struct Emitter {
    /// Function/decl body under assembly (header assembled last — bug #1).
    body: String,
    indent: usize,
    /// Package roots used anywhere in the body → import lines (sorted).
    imports: BTreeSet<String>,
    /// Explicit package root → full Go import path mapping from `Decl::Import`.
    explicit_imports: BTreeMap<String, String>,
    /// Per-function binding hints. A flat map per function: MVP-0's hints
    /// are bound at fn-body top level, and closure bodies share the frame.
    bindings: BTreeMap<String, BindingKind>,
    /// Uniqueness counter for desugar temps (`hyErr0`, `hyErr1`, …).
    temp_n: usize,
    /// Inside a main-wrapper closure? (bare `return` legality, `?` shape)
    in_main_closure: bool,
    /// Inside a function literal (closure) — `?` binds to ITS results.
    fnlit_depth: usize,
    /// Enclosing `for`/`loop` depth in the CURRENT function/closure frame.
    /// A labeled `break loop`/`continue loop` at depth 0 has no Go label
    /// target (labels do not cross func boundaries) — emit refuses instead
    /// of emitting `break hyLoop` against an undefined label (backstop for
    /// closure bodies, which the Tier-0 flat walk skips).
    for_depth: usize,
    /// Dotted associated-fn names declared in this file
    /// (`lru_cache.new`) — call sites mangle to `lru_cache_new[...]`.
    assoc_fns: BTreeSet<String>,
    /// The enclosing fn's single result type IF it is an option (`option[v]`)
    /// — `return none` lowers to that type's zero value.
    cur_option_result: Option<String>,
    /// Go label of the loop currently open in THIS function (innermost last).
    /// `hyLoop` is a fixed name, so two sibling loops that each own a labeled
    /// `break loop` would emit two `hyLoop:` declarations and the module would
    /// fail to build (`label hyLoop already declared`). Each labeled loop
    /// pushes its own name (`hyLoop`, `hyLoop1`, …), and the labeled
    /// break/continue about to render takes the innermost one.
    loop_labels: Vec<String>,
    /// Per-function counter handing each labeled loop a distinct name
    /// (`hyLoop`, `hyLoop1`, …) so sibling loops never declare two labels
    /// with the same name.
    loop_label_n: usize,
    /// Map-slot loans (O8): `x := &m[k]` cannot be a Go map-index address,
    /// so it lowers to a COPY read now + store-back at the loan's block end
    /// (O9's honesty: Go-heap reads are copies). Frames are block-scoped:
    /// a loan flushes at the end of the block that declared it. Each entry
    /// is (loan variable, its store-back statement): `m[k] = n`, or
    /// `m[k] = *n` when `n` is a pointer-typed binding.
    loan_frames: Vec<Vec<(String, String)>>,
    /// #546: outer slot loans handed down into the branches/blocks of a
    /// compound statement (`if`, `match`, `select`, `block`, `managed`, `for`)
    /// whose body contains a concurrency boundary or invalidating write on
    /// the loan's base map. Every branch body starts with these loans in its
    /// frame (and a missing `else` / match fallthrough gets a synthetic
    /// store-back), so the store-back runs on each path before any move or
    /// delete instead of once after the whole compound statement.
    branch_inherit: Option<Vec<(String, String)>>,
    /// E11 handler contract: inside a closure that converts to a
    /// propagation-carrying target (`http.handler`), `?` fires
    /// `http.Error(w, err, 400)` + return instead of the E3 error-return.
    /// Some(writer param name) while emitting such a closure.
    handler_writer: Option<String>,
    /// While emitting a conversion argument (`http.handler(fn(w, r) { … })`)
    /// the target's descriptor, so untyped fn-lit params take the TARGET's
    /// types. Set immediately before the FnLit is emitted, cleared after.
    pending_fnlit_conv: Option<&'static Conversion>,
    /// The enclosing top-level fn's single result type, IF it is a
    /// conversion target (`fn logging(…) http.handler`) — `return fn(…)`
    /// wraps in the conversion (`return http.HandlerFunc(func…)`).
    cur_fn_conv_result: Option<&'static str>,
    /// E3 expression-form hoists (Batch G): Try span start → the hoisted
    /// temp. emit_expr's Try branch substitutes the temp; cleared per fn.
    try_temps: BTreeMap<(u32, u32), String>,
    /// Hoisted expression temps (keyed by AST node address) for short-circuit
    /// `&&`/`||` containing `?` and preceding operands whose evaluation order
    /// must be preserved before a hoisted `?`.
    expr_temps: BTreeMap<usize, String>,
    /// Package mode (example 09): the emitted `package` clause, the prefix
    /// binding-package imports use, and whether `pub fn` exports.
    package_name: String,
    binding_prefix: String,
    export_pub: bool,
    /// The current (non-main) fn's per-result RETURN texts when its results
    /// end in `error` — E3 propagates via `return <zeros…, err>`.
    cur_fn_error_returns: Option<Vec<String>>,
    /// Per-fn result shapes, collected eagerly (Batch I): name →
    /// (n_results, ends_in_error, first_result's binding kind, per-param
    /// pointer-base types). Drives the `?` hoist arity (13:
    /// `summary(doc)?`), the binding-kind registration for calls whose
    /// result is a known handle type (serde_json.value) — `s.set(...)`
    /// normalizes like any rust-call — and the pointer-param auto-address
    /// at call sites (09: `scale_then_register(e, …)` with `e *env`).
    fn_shapes: BTreeMap<String, FnShape>,
    /// User-struct field types (Batch I): struct → (field, declared hy type
    /// name — dotted for shape-known fields, "" otherwise). Receiver typing
    /// for `stats.mu.lock()` (18).
    struct_fields: BTreeMap<String, Vec<(String, String)>>,
    /// var → user-struct name (value or pointer): `stats := pool_stats{}` /
    /// `worker(..., stats *pool_stats)`.
    var_struct: BTreeMap<String, String>,
    /// var → (shape-known dotted type, is_pointer): `var hits atomic.int64`
    /// / `hits *atomic.int64` — the MethodFunc receiver's address-of form.
    var_ty: BTreeMap<String, (String, bool)>,
    /// Variables known to hold a pointer type in the current function frame.
    pointer_vars: BTreeSet<String>,
    /// Local/parameter/receiver/result names bound in the current function frame.
    local_vars: BTreeSet<String>,
    /// Functions whose first result is a pointer type.
    ptr_returning_fns: BTreeSet<String>,
    /// N5 (zeroinit.rs): non-generic user struct → its declared fields,
    /// and user type alias → its target, for zero values that allocate.
    n5_structs: BTreeMap<String, Vec<FieldDecl>>,
    n5_aliases: BTreeMap<String, TypeExpr>,
}

pub(crate) struct ScopeSnapshot {
    local_vars: BTreeSet<String>,
    bindings: BTreeMap<String, BindingKind>,
    var_ty: BTreeMap<String, (String, bool)>,
    var_struct: BTreeMap<String, String>,
    pointer_vars: BTreeSet<String>,
}

impl Emitter {
    pub fn new() -> Self {
        Emitter {
            body: String::new(),
            indent: 0,
            imports: BTreeSet::new(),
            bindings: BTreeMap::new(),
            temp_n: 0,
            in_main_closure: false,
            fnlit_depth: 0,
            for_depth: 0,
            assoc_fns: BTreeSet::new(),
            cur_option_result: None,
            loop_labels: Vec::new(),
            loop_label_n: 0,
            loan_frames: Vec::new(),
            branch_inherit: None,
            handler_writer: None,
            pending_fnlit_conv: None,
            cur_fn_conv_result: None,
            try_temps: BTreeMap::new(),
            expr_temps: BTreeMap::new(),
            package_name: "main".to_string(),
            binding_prefix: "hybrid.program".to_string(),
            export_pub: false,
            cur_fn_error_returns: None,
            fn_shapes: BTreeMap::new(),
            struct_fields: BTreeMap::new(),
            var_struct: BTreeMap::new(),
            var_ty: BTreeMap::new(),
            pointer_vars: BTreeSet::new(),
            local_vars: BTreeSet::new(),
            ptr_returning_fns: BTreeSet::new(),
            explicit_imports: BTreeMap::new(),
            n5_structs: BTreeMap::new(),
            n5_aliases: BTreeMap::new(),
        }
    }

    pub(crate) fn is_unshadowed_pkg_root(&self, name: &str) -> bool {
        !self.local_vars.contains(name) && is_package_root(name)
    }

    pub(crate) fn save_scope(&self) -> ScopeSnapshot {
        ScopeSnapshot {
            local_vars: self.local_vars.clone(),
            bindings: self.bindings.clone(),
            var_ty: self.var_ty.clone(),
            var_struct: self.var_struct.clone(),
            pointer_vars: self.pointer_vars.clone(),
        }
    }

    pub(crate) fn restore_scope(&mut self, saved: ScopeSnapshot) {
        self.local_vars = saved.local_vars;
        self.bindings = saved.bindings;
        self.var_ty = saved.var_ty;
        self.var_struct = saved.var_struct;
        self.pointer_vars = saved.pointer_vars;
    }

    pub(crate) fn shadow_local_name(&mut self, name: &str) {
        if name != "_" {
            self.local_vars.insert(name.to_string());
            self.bindings.remove(name);
            self.var_ty.remove(name);
            self.var_struct.remove(name);
            self.pointer_vars.remove(name);
        }
    }

    pub(crate) fn shadow_local_names(&mut self, names: &[String]) {
        for n in names {
            self.shadow_local_name(n);
        }
    }

    fn next_temp(&mut self, stem: &str) -> String {
        self.temp_n += 1;
        format!("{}{}", stem, self.temp_n - 1)
    }

    fn use_package(&mut self, root: &str) {
        self.imports.insert(root.to_string());
    }

    fn push(&mut self, line: &str) {
        for _ in 0..self.indent {
            self.body.push('\t');
        }
        self.body.push_str(line);
        self.body.push('\n');
    }

    fn push_raw(&mut self, text: &str) {
        self.body.push_str(text);
    }

    // -----------------------------------------------------------------------
    // File assembly (TWO-PASS — the bug-#1 fix lives here)
    // -----------------------------------------------------------------------

    fn emit_file(&mut self, file: &File) -> Result<String, EmitError> {
        // Associated-fn registry (call sites mangle `pkg.name[...]` to
        // `pkg_name[...]` — Go has no dotted identifiers).
        self.assoc_fns = file
            .fns()
            .filter(|f| f.name.contains('.'))
            .map(|f| f.name.clone())
            .collect();

        // Eager tables (Batch I): user-struct field types + per-fn result
        // shapes. Both are needed BEFORE pass 1 — calls type-resolve against
        // fns declared later in the file.
        self.struct_fields = file
            .decls
            .iter()
            .filter_map(|d| match d {
                Decl::Struct(s) => Some((
                    s.name.clone(),
                    s.fields
                        .iter()
                        .map(|f| (f.name.clone(), base_ty_name(&f.ty).unwrap_or_default()))
                        .collect::<Vec<_>>(),
                )),
                _ => None,
            })
            .collect();
        self.n5_structs = file
            .decls
            .iter()
            .filter_map(|d| match d {
                Decl::Struct(s) if s.type_params.is_empty() => {
                    Some((s.name.clone(), s.fields.clone()))
                }
                _ => None,
            })
            .collect();
        self.n5_aliases = file
            .decls
            .iter()
            .filter_map(|d| match d {
                Decl::TypeAlias(t) => Some((t.name.clone(), t.ty.clone())),
                _ => None,
            })
            .collect();
        self.ptr_returning_fns = file
            .fns()
            .filter(|f| {
                matches!(
                    f.results.first(),
                    Some(FnResult {
                        ty: TypeExpr::Pointer { .. },
                        ..
                    })
                )
            })
            .map(|f| f.name.clone())
            .collect();
        self.fn_shapes = file
            .fns()
            .map(|f| {
                let err = matches!(
                    f.results.last(),
                    Some(FnResult {
                        ty: TypeExpr::Named { name, .. },
                        ..
                    }) if name == "error"
                );
                let kind = f
                    .results
                    .first()
                    .and_then(|r| self.binding_kind_of_ty(&r.ty));
                let ptr_params = f
                    .params
                    .iter()
                    .map(|p| match &p.ty {
                        TypeExpr::Pointer { to, .. } => base_ty_name(to),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                (f.name.clone(), (f.results.len(), err, kind, ptr_params))
            })
            .collect();

        // Pass 1: render every declaration into the body buffer, collecting
        // used packages as we go. Top-level decls separate with a blank line
        // (gofmt style) — cheap here, before the header is assembled.
        let mut first_decl = true;
        for decl in &file.decls {
            if first_decl {
                first_decl = false;
            } else if !self.body.is_empty() && !self.body.ends_with("\n\n") {
                // The emptiness guard matters: a leading `import` decl
                // records itself and emits NOTHING, so the next emitting
                // decl must not inherit a stray blank at body start (it
                // rendered as a double blank after the import block).
                self.body.push('\n');
            }
            match decl {
                Decl::Import { path, .. } => {
                    // Explicit imports are recorded; rust/ paths need the
                    // Phase-2 bridge and get a clean error, never a wrong
                    // emission.
                    if let Some(crate_name) = path.strip_prefix("rust/") {
                        // Known pinned crates (M6): the binding packages are
                        // assembled into the build workspace by `hyb build`;
                        // imports register at call sites (use_package).
                        if lookup_rust_crate(crate_name).is_none() {
                            // Known to the checker but unshipped (the audit's
                            // flate2 row): the honest message, not a
                            // "not yet supported" that reads like a language
                            // gap.
                            if lookup_unbound_crate(crate_name).is_some() {
                                return Err(EmitError::unbound_crate(crate_name));
                            }
                            return Err(EmitError::unsupported(format!(
                                "rust crate import `{}` is not in the pinned binding set",
                                crate_name
                            )));
                        }
                        continue;
                    }
                    // A dotted Go path maps to its ROOT for the usage-driven
                    // import set ("net/http" → root "http"; the body says
                    // `http.X`, and import_path_of maps the line back).
                    let root = path.split('/').next_back().unwrap_or(path);
                    self.explicit_imports.insert(root.to_string(), path.clone());
                    self.imports.insert(root.to_string());
                }
                Decl::Const(c) => self.emit_file_const(c)?,
                Decl::Struct(s) => self.emit_struct(s)?,
                Decl::Enum(e) => {
                    return Err(EmitError::unsupported(format!(
                        "enum `{}` (match/enum emission lands with the match surface)",
                        e.name
                    )));
                }
                Decl::TypeAlias(t) => {
                    let ty = self.type_text(&t.ty);
                    self.push(&format!("type {} = {}", t.name, ty));
                }
                Decl::Fn(f) => self.emit_fn(f)?,
            }
        }

        // Pass 2: assemble package + imports + prelude + body. Imports always
        // sit between `package` and the first declaration — Go's required
        // order. The prelude (option/set/hy_pop) lands between imports and
        // the first user decl, only when the emitted body uses it.
        let mut out = String::from("package ");
        out.push_str(&self.package_name);
        out.push('\n');
        let active_imports: Vec<&String> = self
            .imports
            .iter()
            .filter(|root| {
                // An explicit `import "errors"` whose only Hybrid call was
                // `errors.wrap(err, msg)` lowers to `fmt.Errorf(...)` and
                // leaves no `errors.` reference in the emitted Go body (#657).
                !(root.as_str() == "errors" && !self.body.contains("errors."))
            })
            .collect();
        if !active_imports.is_empty() {
            out.push_str("\nimport (\n");
            // Roots map to their real Go import paths (http → net/http,
            // SPEC M5's curated allowlist); binding packages live under the
            // configured prefix (hybrid.program for program builds, the
            // consumer's package path in package mode). The CODE keeps the
            // root spelling — Go's package name for those imports.
            for root in active_imports {
                let path = if let Some(p) = self.explicit_imports.get(root) {
                    p.clone()
                } else if shapes::RUST_CRATES
                    .iter()
                    .any(|c| c.binding_root == root.as_str())
                {
                    format!("{}/{}", self.binding_prefix, root)
                } else {
                    import_path_of(root)
                };
                out.push_str(&format!("\t\"{}\"\n", path));
            }
            out.push_str(")\n");
        }
        let body = &self.body;
        let needs_option = body.contains("option[") || body.contains("some(");
        let needs_set = body.contains("set[")
            || body.contains(".remove(")
            || body.contains(".contains(")
            || body.contains(".insert(");
        let needs_pop = body.contains("hy_pop(");
        if needs_option || needs_set || needs_pop {
            out.push('\n');
            if needs_option {
                out.push_str(OPTION_PRELUDE);
                out.push('\n');
            }
            if needs_set {
                out.push_str(SET_PRELUDE);
                out.push('\n');
            }
            if needs_pop {
                out.push_str(POP_PRELUDE);
                out.push('\n');
            }
        }
        out.push('\n');
        out.push_str(&self.body);
        Ok(out)
    }

    fn emit_file_const(&mut self, c: &ConstDecl) -> Result<(), EmitError> {
        let value = self.emit_expr(&c.value, 0)?;
        // Go's `const` admits only bool/string/number literals. Composite
        // values (slices/maps/structs — 17's `const thresholds = [10, …]`,
        // 18's seed_urls) lower to `var`: same identifier surface, the
        // immutability contract is the checker's business, not Go's.
        let composite = matches!(
            Self::peel_parens(&c.value),
            Expr::SliceLit { .. } | Expr::MapLit { .. } | Expr::StructLit { .. }
        );
        let kw = if composite { "var" } else { "const" };
        match &c.ty {
            Some(t) => {
                let ty = self.type_text(t);
                self.push(&format!("{} {} {} = {}", kw, c.name, ty, value));
            }
            None => self.push(&format!("{} {} = {}", kw, c.name, value)),
        }
        Ok(())
    }

    fn emit_struct(&mut self, s: &StructDecl) -> Result<(), EmitError> {
        if !s.type_params.is_empty() {
            // Generic struct (example-07 surface): Go requires a constraint on
            // every type parameter (`type box[t any] struct {`). Without a
            // constraint, `type box[t] struct {` is parsed by Go as an array
            // type `[t]struct{}` (`undefined: t`, issue #722).
            let param_names: BTreeSet<String> =
                s.type_params.iter().map(|p| p.name.clone()).collect();
            let mut comparable_params = BTreeSet::new();
            for f in &s.fields {
                Self::collect_comparable_in_ty(&f.ty, &param_names, &mut comparable_params);
            }
            let params = self.format_type_params(&s.type_params, &comparable_params);
            self.push(&format!("type {}[{}] struct {{", s.name, params));
        } else {
            self.push(&format!("type {} struct {{", s.name));
        }
        self.indent += 1;
        // gofmt aligns consecutive field declarations: names padded to the
        // longest, types in one column. Do it here so emitted output is
        // gofmt-canonical without a post-pass.
        let width = s.fields.iter().map(|f| f.name.len()).max().unwrap_or(0);
        for f in &s.fields {
            // Our own struct fields stay snake_case (M4 normalizes FOREIGN
            // names, never ours).
            let ty = self.type_text(&f.ty);
            self.push(&format!("{:<width$} {}", f.name, ty, width = width));
        }
        self.indent -= 1;
        self.push("}");
        Ok(())
    }

    fn constraint_text(&self, c: &str) -> String {
        // `comparable` / `any` are Go's spellings already.
        c.to_string()
    }

    /// Format a type parameter list for Go emission, supplying a constraint
    /// (`comparable` when used as a map key or set element, inherited from a
    /// grouped trailing constraint `[a, b comparable]`, or `any` by default)
    /// whenever the Hybrid source omits one (#722).
    fn format_type_params(
        &self,
        type_params: &[TypeParam],
        comparable_params: &BTreeSet<String>,
    ) -> String {
        type_params
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let constraint = match &p.constraint {
                    Some(c) => self.constraint_text(c),
                    None => {
                        if comparable_params.contains(&p.name) {
                            "comparable".to_string()
                        } else if let Some(inherited) = type_params[i + 1..]
                            .iter()
                            .find_map(|next| next.constraint.as_deref())
                        {
                            self.constraint_text(inherited)
                        } else {
                            "any".to_string()
                        }
                    }
                };
                format!("{} {}", p.name, constraint)
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn collect_comparable_in_ty(
        ty: &TypeExpr,
        param_names: &BTreeSet<String>,
        out: &mut BTreeSet<String>,
    ) {
        match ty {
            TypeExpr::Map { key, value, .. } => {
                if let TypeExpr::Named { name, .. } = key.as_ref() {
                    if param_names.contains(name) {
                        out.insert(name.clone());
                    }
                }
                Self::collect_comparable_in_ty(key, param_names, out);
                Self::collect_comparable_in_ty(value, param_names, out);
            }
            TypeExpr::Named { name, .. } => {
                if let Some(inner) = name.strip_prefix("set[").and_then(|s| s.strip_suffix(']')) {
                    let elem = inner.trim();
                    if param_names.contains(elem) {
                        out.insert(elem.to_string());
                    }
                }
            }
            TypeExpr::Pointer { to, .. } => Self::collect_comparable_in_ty(to, param_names, out),
            TypeExpr::Slice { elem, .. }
            | TypeExpr::Array { elem, .. }
            | TypeExpr::Chan { elem, .. }
            | TypeExpr::Option { elem, .. } => {
                Self::collect_comparable_in_ty(elem, param_names, out);
            }
            TypeExpr::Fn {
                params, results, ..
            } => {
                for p in params {
                    Self::collect_comparable_in_ty(&p.ty, param_names, out);
                }
                for r in results {
                    Self::collect_comparable_in_ty(r, param_names, out);
                }
            }
        }
    }

    fn collect_comparable_in_block(
        b: &Block,
        param_names: &BTreeSet<String>,
        out: &mut BTreeSet<String>,
    ) {
        for s in &b.stmts {
            Self::collect_comparable_in_stmt(s, param_names, out);
        }
    }

    fn collect_comparable_in_stmt(
        s: &Stmt,
        param_names: &BTreeSet<String>,
        out: &mut BTreeSet<String>,
    ) {
        match s {
            Stmt::Local(l) => {
                if let Some(ty) = &l.ty {
                    Self::collect_comparable_in_ty(ty, param_names, out);
                }
                for v in &l.values {
                    Self::collect_comparable_in_expr(v, param_names, out);
                }
            }
            Stmt::Assign { values, .. } | Stmt::Return { values, .. } => {
                for v in values {
                    Self::collect_comparable_in_expr(v, param_names, out);
                }
            }
            Stmt::Expr { expr, .. } | Stmt::Defer { call: expr, .. } => {
                Self::collect_comparable_in_expr(expr, param_names, out);
            }
            Stmt::Send { chan, value, .. } => {
                Self::collect_comparable_in_expr(chan, param_names, out);
                Self::collect_comparable_in_expr(value, param_names, out);
            }
            Stmt::If {
                init,
                cond,
                then,
                otherwise,
                ..
            } => {
                if let Some(i) = init {
                    Self::collect_comparable_in_stmt(i, param_names, out);
                }
                Self::collect_comparable_in_expr(cond, param_names, out);
                Self::collect_comparable_in_block(then, param_names, out);
                if let Some(o) = otherwise {
                    Self::collect_comparable_in_stmt(o, param_names, out);
                }
            }
            Stmt::For { kind, body, .. } => {
                match kind {
                    ForKind::While { cond } => {
                        Self::collect_comparable_in_expr(cond, param_names, out);
                    }
                    ForKind::Range { lo, hi, .. } => {
                        Self::collect_comparable_in_expr(lo, param_names, out);
                        Self::collect_comparable_in_expr(hi, param_names, out);
                    }
                    ForKind::Iterate { expr, .. } => {
                        Self::collect_comparable_in_expr(expr, param_names, out);
                    }
                    ForKind::Infinite => {}
                }
                Self::collect_comparable_in_block(body, param_names, out);
            }
            Stmt::Block { block, .. } | Stmt::Managed { body: block, .. } => {
                Self::collect_comparable_in_block(block, param_names, out);
            }
            Stmt::Task { callee, args, .. } => {
                Self::collect_comparable_in_expr(callee, param_names, out);
                for a in args {
                    Self::collect_comparable_in_expr(&a.value, param_names, out);
                }
            }
            Stmt::Match { subject, arms, .. } => {
                Self::collect_comparable_in_expr(subject, param_names, out);
                for a in arms {
                    if let Some(g) = &a.guard {
                        Self::collect_comparable_in_expr(g, param_names, out);
                    }
                    Self::collect_comparable_in_block(&a.body, param_names, out);
                }
            }
            Stmt::Select { arms, .. } => {
                for a in arms {
                    Self::collect_comparable_in_block(&a.body, param_names, out);
                }
            }
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
        }
    }

    fn collect_comparable_in_expr(
        e: &Expr,
        param_names: &BTreeSet<String>,
        out: &mut BTreeSet<String>,
    ) {
        match e {
            Expr::MapLit { key, value, entries, .. } => {
                if let TypeExpr::Named { name, .. } = key.as_ref() {
                    if param_names.contains(name) {
                        out.insert(name.clone());
                    }
                }
                Self::collect_comparable_in_ty(key, param_names, out);
                Self::collect_comparable_in_ty(value, param_names, out);
                for (k, v) in entries {
                    Self::collect_comparable_in_expr(k, param_names, out);
                    Self::collect_comparable_in_expr(v, param_names, out);
                }
            }
            Expr::StructLit { ty, fields, .. } => {
                if let Some(t) = ty {
                    Self::collect_comparable_in_ty(t, param_names, out);
                }
                for f in fields {
                    Self::collect_comparable_in_expr(&f.value, param_names, out);
                }
            }
            Expr::SliceLit { elem, items, .. } => {
                if let Some(t) = elem {
                    Self::collect_comparable_in_ty(t, param_names, out);
                }
                for i in items {
                    Self::collect_comparable_in_expr(i, param_names, out);
                }
            }
            Expr::Call { callee, args, .. } => {
                Self::collect_comparable_in_expr(callee, param_names, out);
                for a in args {
                    Self::collect_comparable_in_expr(&a.value, param_names, out);
                }
            }
            Expr::Binary { lhs, rhs, .. } => {
                Self::collect_comparable_in_expr(lhs, param_names, out);
                Self::collect_comparable_in_expr(rhs, param_names, out);
            }
            Expr::Unary { operand, .. }
            | Expr::AddrOf { operand, .. }
            | Expr::Move { operand, .. }
            | Expr::Try { operand, .. }
            | Expr::Paren { inner: operand, .. } => {
                Self::collect_comparable_in_expr(operand, param_names, out);
            }
            Expr::Receive { chan, .. } => {
                Self::collect_comparable_in_expr(chan, param_names, out);
            }
            Expr::Index { base, index, .. } => {
                Self::collect_comparable_in_expr(base, param_names, out);
                Self::collect_comparable_in_expr(index, param_names, out);
            }
            Expr::Slice { base, lo, hi, .. } => {
                Self::collect_comparable_in_expr(base, param_names, out);
                if let Some(l) = lo {
                    Self::collect_comparable_in_expr(l, param_names, out);
                }
                if let Some(h) = hi {
                    Self::collect_comparable_in_expr(h, param_names, out);
                }
            }
            Expr::Field { base, .. } => {
                Self::collect_comparable_in_expr(base, param_names, out);
            }
            Expr::FnLit {
                params,
                results,
                body,
                ..
            } => {
                for p in params {
                    Self::collect_comparable_in_ty(&p.ty, param_names, out);
                }
                for r in results {
                    Self::collect_comparable_in_ty(&r.ty, param_names, out);
                }
                Self::collect_comparable_in_block(body, param_names, out);
            }
            Expr::Interp { parts, .. } => {
                for p in parts {
                    if let InterpPart::Expr(x) = p {
                        Self::collect_comparable_in_expr(x, param_names, out);
                    }
                }
            }
            Expr::Ident { .. } | Expr::Lit { .. } => {}
        }
    }

    // -----------------------------------------------------------------------
    // Functions (incl. the main wrapper — the E6 exit contract)
    // -----------------------------------------------------------------------

    fn emit_fn(&mut self, f: &FnDecl) -> Result<(), EmitError> {
        self.bindings.clear();
        self.var_struct.clear();
        self.var_ty.clear();
        self.pointer_vars.clear();
        self.local_vars.clear();
        self.temp_n = 0;
        self.try_temps.clear();
        self.loop_labels.clear();
        self.loop_label_n = 0;
        self.expr_temps.clear();

        // Associated functions (`lru_cache.new`) mangle to
        // `lru_cache_new` — Go has no dotted identifiers. Call sites mangle
        // identically (see emit_call). Package mode exports `pub fn` via
        // M4's export mapping (example 09: validate_email → ValidateEmail).
        let go_name =
            if self.export_pub && f.is_pub && f.receiver.is_none() && !f.name.contains('.') {
                to_go_name(&f.name)
            } else {
                f.name.replace('.', "_")
            };
        let is_main = f.name == "main" && f.receiver.is_none();
        // E4/E6: main's `?` sites trigger the exit contract. If main declares
        // results OR contains top-level `?` sites, the body moves into a
        // closure whose error result drives the wrapper.
        let body_has_try = f
            .body
            .as_ref()
            .map(|b| self.block_has_try(b))
            .unwrap_or(false);
        let main_needs_wrapper = is_main && (!f.results.is_empty() || body_has_try);

        // `return none` lowers to the declared option result's zero value.
        self.cur_option_result = None;
        // Batch E: a single conversion-target result (`fn logging(…) http.handler`)
        // makes `return fn(…)` wrap in the conversion (HandlerFunc).
        self.cur_fn_conv_result = None;
        if f.results.len() == 1 && f.results[0].name.is_none() {
            let rt = self.type_text(&f.results[0].ty);
            if rt.starts_with("option[") {
                self.cur_option_result = Some(rt);
            }
            if let Some(conv) = self.result_conv_key(&f.results[0].ty) {
                self.cur_fn_conv_result = Some(conv);
            }
        }

        // E3 in an error-result fn (package mode, example 09): the
        // propagation tail returns the zero results + the error.
        self.cur_fn_error_returns = None;
        if !is_main && !f.results.is_empty() && !main_needs_wrapper {
            let last_is_err = matches!(
                &f.results.last().unwrap().ty,
                TypeExpr::Named { name, .. } if name == "error"
            );
            if last_is_err {
                let mut texts = Vec::new();
                for (i, r) in f.results.iter().enumerate() {
                    if i == f.results.len() - 1 {
                        texts.push(String::new()); // filled by propagate_error
                    } else {
                        texts.push(self.zero_value(&r.ty));
                    }
                }
                self.cur_fn_error_returns = Some(texts);
            }
        }

        for r in &f.results {
            if let Some(name) = &r.name {
                if name != "_" {
                    self.shadow_local_name(name);
                    if matches!(r.ty, TypeExpr::Pointer { .. }) {
                        self.pointer_vars.insert(name.clone());
                    }
                    if let Some(kind) = self.binding_kind_of_ty(&r.ty) {
                        self.bindings.insert(name.clone(), kind);
                    }
                    self.register_var_ty(name, &r.ty);
                }
            }
        }

        // Hints from parameter types (dotted foreign types → Foreign).
        if let Some(recv) = &f.receiver {
            if recv.name != "_" {
                self.local_vars.insert(recv.name.clone());
            }
            if recv.is_pointer {
                self.pointer_vars.insert(recv.name.clone());
            }
            if let Some(kind) = self.binding_kind_of_ty(&recv.ty) {
                self.bindings.insert(recv.name.clone(), kind);
            }
            self.register_var_ty(&recv.name, &recv.ty);
            if recv.is_pointer {
                if let Some((_, ptr)) = self.var_ty.get_mut(&recv.name) {
                    *ptr = true;
                }
            }
            let recv_ty = self.type_text(&recv.ty);
            let star = if recv.is_pointer { "*" } else { "" };
            self.push_raw(&format!(
                "func ({} {}{}) {}(",
                recv.name, star, recv_ty, go_name
            ));
        } else {
            // Generic fn: `func lru_cache_new[k comparable, v any](...)`.
            if f.type_params.is_empty() {
                self.push_raw(&format!("func {}(", go_name));
            } else {
                let param_names: BTreeSet<String> =
                    f.type_params.iter().map(|p| p.name.clone()).collect();
                let mut comparable_params = BTreeSet::new();
                for p in &f.params {
                    Self::collect_comparable_in_ty(&p.ty, &param_names, &mut comparable_params);
                }
                for r in &f.results {
                    Self::collect_comparable_in_ty(&r.ty, &param_names, &mut comparable_params);
                }
                if let Some(body) = &f.body {
                    Self::collect_comparable_in_block(body, &param_names, &mut comparable_params);
                }
                let params = self.format_type_params(&f.type_params, &comparable_params);
                self.push_raw(&format!("func {}[{}]({}", go_name, params, ""));
            }
        }
        self.emit_params(&f.params);
        self.push_raw(")");

        if main_needs_wrapper {
            // func main() { <ret> := func(<results…>, hyErr error) { … }() … }
            self.push_raw(" {\n");
            self.indent += 1;
            let ret_vars = self.main_ret_vars(f);
            let closure_sig = self.main_closure_sig(f);
            self.push(&format!("{} := func() {} {{", ret_vars, closure_sig));
            self.indent += 1;
            let saved = self.in_main_closure;
            self.in_main_closure = true;
            self.emit_hints_from_params(&f.params);
            if let Some(body) = &f.body {
                // The closure-body level is already pushed above;
                // emit_block_body adds its own, so drop one first or every
                // wrapper body line renders one tab deeper than gofmt.
                self.indent -= 1;
                self.emit_block_body(&body.stmts)?;
                self.indent += 1;
            }
            // Terminal bare `return`: the wrapper closure's named result
            // makes it legal, and a body ending in a non-terminating
            // statement (a range-for — example 06's smoke loop) otherwise
            // fails go build with "missing return". Happy-path fall-through
            // returns hyErr == nil, exactly the E6 contract's ok path.
            self.push("return");
            self.in_main_closure = saved;
            self.indent -= 1;
            self.push("}()");
            self.push("if hyErr != nil {");
            self.indent += 1;
            // E6: defers already ran (closure return) — print + exit 1.
            self.push("fmt.Fprintln(os.Stderr, filepath.Base(os.Args[0])+\":\", hyErr)");
            self.push("os.Exit(1)");
            self.indent -= 1;
            self.push("}");
            if !f.results.is_empty() {
                // The declared non-error result IS the exit code on the
                // explicit `return N` path (example 02's `return 1`). The
                // receiver var mirrors the declared result's name.
                let first_ret = ret_vars.split(", ").next().unwrap_or("hyR0").to_string();
                self.push(&format!("os.Exit({})", first_ret));
            }
            self.indent -= 1;
            self.push("}");
            self.use_package("fmt");
            self.use_package("os");
            self.use_package("path/filepath");
            return Ok(());
        }

        // Plain signature rendering (non-main, or main with no results/`?`).
        // One UNNAMED result renders bare (`func f() int {`); named or
        // multiple results parenthesize (`(code int)`, `(a int, b string)`).
        match f.results.len() {
            0 => self.push_raw(" {\n"),
            1 => {
                let r = &f.results[0];
                let ty = self.type_text(&r.ty);
                match &r.name {
                    Some(n) => self.push_raw(&format!(" ({} {}) {{\n", n, ty)),
                    None => self.push_raw(&format!(" {} {{\n", ty)),
                }
            }
            2 => {
                let results = self.result_list_text(&f.results);
                self.push_raw(&format!(" ({}) {{\n", results));
            }
            n => {
                return Err(EmitError::unsupported(format!(
                    "functions with {} results (max 2: value + error)",
                    n
                )));
            }
        }
        // emit_block_body supplies ONE level for the block it emits (the
        // contract every statement-level block relies on), so a fn body is
        // exactly one tab under its header — gofmt's canonical base. The
        // hints write no lines; no bump is needed anywhere here.
        self.emit_hints_from_params(&f.params);
        if let Some(body) = &f.body {
            self.emit_block_body(&body.stmts)?;
        }
        self.push("}");
        Ok(())
    }

    /// Render a result list without mixing named and unnamed Go parameters.
    /// Besides rejecting `(code int, string)`, Go parses `(int, text string)`
    /// as two named string results, so preserving a mixed Hybrid list verbatim
    /// can change its types. Name every result only when the list is mixed;
    /// fully named and fully unnamed lists retain their existing emission.
    fn result_list_text(&mut self, results: &[FnResult]) -> String {
        let mixed = results.iter().any(|r| r.name.is_some())
            && results.iter().any(|r| r.name.is_none());
        let mut used: BTreeSet<String> = results
            .iter()
            .filter_map(|r| r.name.clone())
            .collect();
        let mut parts = Vec::with_capacity(results.len());
        for (i, r) in results.iter().enumerate() {
            let ty = self.type_text(&r.ty);
            match &r.name {
                Some(n) => parts.push(format!("{} {}", n, ty)),
                None if mixed => {
                    let base = format!("hyR{}", i);
                    let mut name = base.clone();
                    let mut suffix = 0;
                    while used.contains(&name) {
                        suffix += 1;
                        name = format!("{}_{}", base, suffix);
                    }
                    used.insert(name.clone());
                    parts.push(format!("{} {}", name, ty));
                }
                None => parts.push(ty),
            }
        }
        parts.join(", ")
    }

    fn main_ret_vars(&self, f: &FnDecl) -> String {
        if f.results.is_empty() {
            "hyErr".to_string()
        } else {
            // `code` is example-02's named result; unnamed results get a
            // generated receiver var.
            let names: Vec<String> = f
                .results
                .iter()
                .enumerate()
                .map(|(i, r)| r.name.clone().unwrap_or_else(|| format!("hyR{}", i)))
                .collect();
            names.join(", ") + ", hyErr"
        }
    }

    /// The wrapper closure's signature: the declared results (named) plus
    /// the hidden `hyErr error` the `?` desugaring returns through.
    fn main_closure_sig(&mut self, f: &FnDecl) -> String {
        let mut parts: Vec<String> = Vec::new();
        for (i, r) in f.results.iter().enumerate() {
            let n = r.name.clone().unwrap_or_else(|| format!("hyR{}", i));
            let ty = self.type_text(&r.ty);
            parts.push(format!("{} {}", n, ty));
        }
        parts.push("hyErr error".to_string());
        format!("({})", parts.join(", "))
    }

    fn emit_params(&mut self, params: &[Param]) {
        let mut rendered = String::new();
        for (i, p) in params.iter().enumerate() {
            if i > 0 {
                rendered.push_str(", ");
            }
            let ty = self.type_text(&p.ty);
            rendered.push_str(&format!("{} {}", p.name, ty));
        }
        self.push_raw(&rendered);
    }

    fn emit_hints_from_params(&mut self, params: &[Param]) {
        for p in params {
            self.shadow_local_name(&p.name);
            if let Some(kind) = self.binding_kind_of_ty(&p.ty) {
                self.bindings.insert(p.name.clone(), kind);
            } else if matches!(p.ty, TypeExpr::Chan { .. }) {
                self.bindings.insert(p.name.clone(), BindingKind::Chan);
            }
            if matches!(p.ty, TypeExpr::Pointer { .. }) {
                self.pointer_vars.insert(p.name.clone());
            }
            self.register_var_ty(&p.name, &p.ty);
        }
    }

    pub(crate) fn expr_is_pointer(&self, e: &Expr) -> bool {
        match e {
            Expr::AddrOf { .. } => true,
            Expr::Paren { inner, .. } => self.expr_is_pointer(inner),
            Expr::Try { operand, .. } => self.expr_is_pointer(operand),
            Expr::Ident { name, .. } => self.pointer_vars.contains(name),
            Expr::Call { callee, .. } => {
                if let Expr::Ident { name, .. } = callee.as_ref() {
                    self.ptr_returning_fns.contains(name)
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// Receiver typing (Batch I): a var/param whose type is shape-known
    /// dotted (`atomic.int64`) lands in var_ty with its pointer-ness; one
    /// whose type is a user struct (`pool_stats`, value or pointer) lands
    /// in var_struct — `stats.mu.lock()` types through struct_fields.
    fn register_var_ty(&mut self, name: &str, ty: &TypeExpr) {
        let (base, ptr) = match ty {
            TypeExpr::Pointer { to, .. } => (base_ty_name(to), true),
            t => (base_ty_name(t), false),
        };
        let Some(base) = base else { return };
        if base.is_empty() {
            return;
        }
        let struct_base = base.split('[').next().unwrap_or(&base);
        if shapes::SHAPES.iter().any(|(k, _)| *k == base.as_str()) {
            self.var_ty.insert(name.to_string(), (base, ptr));
        } else if self.struct_fields.contains_key(struct_base) {
            self.var_struct
                .insert(name.to_string(), struct_base.to_string());
        }
    }

    /// Binding kind from a TYPE (Batch E): a dotted type that the shape
    /// table knows gives ForeignTy(key) — members resolve field-vs-method;
    /// any other dotted type stays generic Foreign.
    fn binding_kind_of_ty(&self, t: &TypeExpr) -> Option<BindingKind> {
        if matches!(t, TypeExpr::Chan { .. }) {
            return Some(BindingKind::Chan);
        }
        let name = match t {
            TypeExpr::Named { name, .. } => name.as_str(),
            TypeExpr::Pointer { to, .. } => return self.binding_kind_of_ty(to),
            _ => return None,
        };
        if name.starts_with("set[") || name == "set" {
            return Some(BindingKind::Set);
        }
        if name.is_empty() || !name.contains('.') {
            return None;
        }
        if shapes::SHAPES.iter().any(|(k, _)| *k == name) {
            // &'static from the static table — find returns a reference with
            // the static lifetime.
            Some(BindingKind::ForeignTy(
                shapes::SHAPES.iter().find(|(k, _)| *k == name).unwrap().0,
            ))
        } else {
            Some(BindingKind::Foreign)
        }
    }

    /// The conversion-target key of a result type (`http.handler`), if the
    /// type IS one — `return fn(…)` wrapping (Batch E).
    fn result_conv_key(&self, t: &TypeExpr) -> Option<&'static str> {
        let name = match t {
            TypeExpr::Named { name, .. } => name.as_str(),
            _ => return None,
        };
        if lookup_conversion(name).is_some() {
            // Promote to 'static via the CONVERSIONS table's own key.
            shapes::CONVERSIONS
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(k, _)| *k)
        } else {
            None
        }
    }

    fn type_is_foreign(&self, t: &TypeExpr) -> bool {
        match t {
            TypeExpr::Named { name, .. } => name.contains('.'),
            TypeExpr::Pointer { to, .. } => self.type_is_foreign(to),
            TypeExpr::Slice { elem, .. } => self.type_is_foreign(elem),
            TypeExpr::Map { key, value, .. } => {
                self.type_is_foreign(key) || self.type_is_foreign(value)
            }
            _ => false,
        }
    }

    fn block_has_try(&self, b: &Block) -> bool {
        b.stmts.iter().any(|s| self.stmt_has_try(s))
    }

    fn stmt_has_try(&self, s: &Stmt) -> bool {
        match s {
            Stmt::Local(l) => l.values.iter().any(|v| self.expr_has_try(v)),
            Stmt::Assign {
                targets, values, ..
            } => {
                targets.iter().any(Self::place_has_hoistable_try)
                    || values.iter().any(|v| self.expr_has_try(v))
            }
            Stmt::Expr { expr, .. } => self.expr_has_try(expr),
            Stmt::Send { chan, value, .. } => {
                self.expr_has_try(chan) || self.expr_has_try(value)
            }
            Stmt::If {
                init,
                cond,
                then,
                otherwise,
                ..
            } => {
                let init_try = init
                    .as_ref()
                    .map(|i| self.stmt_has_try(i.as_ref()))
                    .unwrap_or(false);
                let else_try = otherwise
                    .as_ref()
                    .map(|o| self.stmt_has_try(o.as_ref()))
                    .unwrap_or(false);
                init_try || self.expr_has_try(cond) || self.block_has_try(then) || else_try
            }
            Stmt::For { kind, body, .. } => {
                let kind_try = match kind {
                    ForKind::While { cond } => self.expr_has_try(cond),
                    ForKind::Range { lo, hi, .. } => self.expr_has_try(lo) || self.expr_has_try(hi),
                    ForKind::Iterate { expr, .. } => self.expr_has_try(expr),
                    ForKind::Infinite => false,
                };
                kind_try || self.block_has_try(body)
            }
            Stmt::Return { values, .. } => values.iter().any(|v| self.expr_has_try(v)),
            Stmt::Defer { call, .. } => self.expr_has_try(call),
            Stmt::Block { block, .. } => self.block_has_try(block),
            Stmt::Managed { body, .. } => self.block_has_try(body),
            Stmt::Task { callee, args, .. } => {
                self.expr_has_try(callee) || args.iter().any(|a| self.expr_has_try(&a.value))
            }
            Stmt::Match { subject, arms, .. } => {
                self.expr_has_try(subject)
                    || arms.iter().any(|a| {
                        a.guard
                            .as_ref()
                            .map(|g| self.expr_has_try(g))
                            .unwrap_or(false)
                            || self.block_has_try(&a.body)
                    })
            }
            Stmt::Select { arms, .. } => arms.iter().any(|a| self.block_has_try(&a.body)),
            Stmt::Break { .. } | Stmt::Continue { .. } => false,
        }
    }

    fn expr_has_try(&self, e: &Expr) -> bool {
        match e {
            Expr::Try { .. } => true,
            Expr::Binary { lhs, rhs, .. } => self.expr_has_try(lhs) || self.expr_has_try(rhs),
            Expr::Unary { operand, .. }
            | Expr::AddrOf { operand, .. }
            | Expr::Move { operand, .. } => self.expr_has_try(operand),
            Expr::Receive { chan, .. } => self.expr_has_try(chan),
            Expr::Index { base, index, .. } => self.expr_has_try(base) || self.expr_has_try(index),
            Expr::Slice { base, lo, hi, .. } => {
                self.expr_has_try(base)
                    || lo.as_ref().map(|x| self.expr_has_try(x)).unwrap_or(false)
                    || hi.as_ref().map(|x| self.expr_has_try(x)).unwrap_or(false)
            }
            Expr::Field { base, .. } => self.expr_has_try(base),
            Expr::Call { callee, args, .. } => {
                self.expr_has_try(callee) || args.iter().any(|a| self.expr_has_try(&a.value))
            }
            Expr::StructLit { fields, .. } => fields.iter().any(|f| self.expr_has_try(&f.value)),
            Expr::SliceLit { items, .. } => items.iter().any(|i| self.expr_has_try(i)),
            Expr::MapLit { entries, .. } => entries
                .iter()
                .any(|(k, v)| self.expr_has_try(k) || self.expr_has_try(v)),
            Expr::Interp { parts, .. } => parts.iter().any(|p| match p {
                InterpPart::Expr(x) => self.expr_has_try(x),
                InterpPart::Lit(_) => false,
            }),
            Expr::FnLit { body, .. } => self.block_has_try(body),
            Expr::Paren { inner, .. } => self.expr_has_try(inner),
            _ => false,
        }
    }

    // -----------------------------------------------------------------------
    // Types (Go spelling)
    // -----------------------------------------------------------------------

    pub fn type_text(&mut self, t: &TypeExpr) -> String {
        match t {
            TypeExpr::Named { name, .. } => self.go_type_name(name),
            TypeExpr::Pointer { to, .. } => format!("*{}", self.type_text(to)),
            TypeExpr::Slice { elem, .. } => format!("[]{}", self.type_text(elem)),
            TypeExpr::Array { len, elem, .. } => match len {
                Some(e) => {
                    let n = self.expr_text_static(e);
                    format!("[{}]{}", n, self.type_text(elem))
                }
                None => format!("[...]{}", self.type_text(elem)),
            },
            TypeExpr::Map { key, value, .. } => {
                format!("map[{}]{}", self.type_text(key), self.type_text(value))
            }
            TypeExpr::Chan { dir, elem, .. } => {
                let e = self.type_text(elem);
                match dir {
                    ChanDir::Both => format!("chan {}", e),
                    ChanDir::Recv => format!("<-chan {}", e),
                    ChanDir::Send => format!("chan<- {}", e),
                }
            }
            TypeExpr::Option { elem, .. } => {
                // option[T] — the prelude struct (val + ok); `none` is the
                // zero value, `some(v)` the constructor (see OPTION_PRELUDE).
                format!("option[{}]", self.type_text(elem))
            }
            TypeExpr::Fn {
                params, results, ..
            } => {
                let all_named = params.iter().all(|p| {
                    !p.name.is_empty()
                        && !matches!(&p.ty, TypeExpr::Named { name, .. } if name.is_empty())
                });
                let ps = params
                    .iter()
                    .map(|p| {
                        let t = self.type_text(&p.ty);
                        if p.name.is_empty() {
                            t
                        } else if t.is_empty() {
                            self.go_type_name(&p.name)
                        } else if all_named {
                            format!("{} {}", p.name, t)
                        } else {
                            t
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let rs = results
                    .iter()
                    .map(|t| self.type_text(t))
                    .collect::<Vec<_>>()
                    .join(", ");
                match results.len() {
                    0 => format!("func({})", ps),
                    1 => format!("func({}) {}", ps, rs),
                    _ => format!("func({}) ({})", ps, rs),
                }
            }
        }
    }

    /// One type NAME in Go spelling. Dotted = foreign (`http.request` →
    /// `http.Request`, import registered). Undotted = ours or a Go builtin —
    /// verbatim (never CamelCased: `node_id` and `int` both stay).
    /// Also translates folded type arguments (`box[u8]` → `box[uint8]`,
    /// `box[http.request]` → `box[http.Request]`, issue #722).
    pub(crate) fn go_type_name(&mut self, name: &str) -> String {
        let name = name.trim();
        if let Some(rest) = name.strip_prefix('*') {
            return format!("*{}", self.go_type_name(rest));
        }
        if let Some(rest) = name.strip_prefix("[]") {
            return format!("[]{}", self.go_type_name(rest));
        }
        if let Some(rest) = name.strip_prefix("[...]") {
            return format!("[...]{}", self.go_type_name(rest));
        }
        if let Some(rest) = name.strip_prefix("<-chan ") {
            return format!("<-chan {}", self.go_type_name(rest));
        }
        if let Some(rest) = name.strip_prefix("chan<- ") {
            return format!("chan<- {}", self.go_type_name(rest));
        }
        if let Some(rest) = name.strip_prefix("chan ") {
            return format!("chan {}", self.go_type_name(rest));
        }
        if let Some(rest) = name.strip_prefix("map[") {
            if let Some((key_s, val_s)) = Self::split_bracket_prefix(rest) {
                let k = self.go_type_name(key_s);
                let v = self.go_type_name(val_s);
                return format!("map[{}]{}", k, v);
            }
        }
        if let Some(open) = name.find('[') {
            if open > 0 && name.ends_with(']') {
                let head = &name[..open];
                let inner = &name[open + 1..name.len() - 1];
                let go_head = self.go_type_name(head);
                let go_args = Self::split_top_level_type_args(inner)
                    .into_iter()
                    .map(|a| self.go_type_name(a))
                    .collect::<Vec<_>>()
                    .join(", ");
                return format!("{}[{}]", go_head, go_args);
            }
        }
        if let Some((root, rest)) = name.split_once('.') {
            // Rust-owned types map to their binding package's handle type
            // (serde_json.value → jsonbinding.Value) — NOT the naive
            // package spelling, which would import a nonexistent package.
            if let Some((binding_root, go_ty)) = shapes::lookup_rust_type(name) {
                self.use_package(binding_root);
                return format!("{}.{}", binding_root, go_ty);
            }
            self.use_package(root);
            let joined = rest
                .split('.')
                .map(to_go_name)
                .collect::<Vec<_>>()
                .join(".");
            format!("{}.{}", root, joined)
        } else {
            // A SPEC primitive spelling maps to its Go equivalent
            // (`u16` -> `uint16`); anything else stays verbatim. Issue #147.
            match shapes::go_primitive_name(name) {
                Some(go) => go.to_string(),
                None => name.to_string(),
            }
        }
    }

    /// Translate a bracketed type-argument suffix like `"[string, u8]"` into
    /// Go type spellings `"[string, uint8]"`.
    pub(crate) fn translate_type_args_suffix(&mut self, suffix: &str) -> String {
        if let Some(inner) = suffix.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            let args = Self::split_top_level_type_args(inner)
                .into_iter()
                .map(|a| self.go_type_name(a))
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{}]", args)
        } else {
            suffix.to_string()
        }
    }

    fn split_bracket_prefix(s: &str) -> Option<(&str, &str)> {
        let mut depth = 1usize;
        for (i, ch) in s.char_indices() {
            match ch {
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some((&s[..i], &s[i + 1..]));
                    }
                }
                _ => {}
            }
        }
        None
    }

    pub(crate) fn split_top_level_type_args(s: &str) -> Vec<&str> {
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

    // -----------------------------------------------------------------------
    // Small helpers
    // -----------------------------------------------------------------------

    /// Zero-value expression for a Go type (the `?` desugar's explicit
    /// returns). `*new(T)` is the universal fallback — a stack zero, no
    /// allocation.
    pub fn zero_value(&mut self, t: &TypeExpr) -> String {
        match t {
            TypeExpr::Named { name, .. } if !name.contains('.') && !name.contains('[') => {
                if self.struct_fields.contains_key(name.as_str()) {
                    return format!("*new({})", name);
                }
                match name.as_str() {
                    "bool" => "false".into(),
                    "string" => "\"\"".into(),
                    // A SPEC primitive spelling takes its own zero value
                    // (`var n u16` -> `var n uint16 = 0`, not `*new(u16)`).
                    // Issue #147.
                    _ if shapes::go_primitive_name(name).is_some() => "0".into(),
                    "byte" | "rune" | "int" | "int8" | "int16" | "int32" | "int64" | "uint"
                    | "uint8" | "uint16" | "uint32" | "uint64" | "uintptr" | "float32"
                    | "float64" | "complex64" | "complex128" => "0".into(),
                    "error" | "any" => "nil".into(),
                    _ => format!("*new({})", name),
                }
            }
            TypeExpr::Slice { .. }
            | TypeExpr::Map { .. }
            | TypeExpr::Chan { .. }
            | TypeExpr::Pointer { .. }
            | TypeExpr::Fn { .. } => "nil".into(),
            TypeExpr::Option { .. } => format!("{}{{}}", self.type_text(t)),
            _ => format!("*new({})", self.type_text(t)),
        }
    }
}

impl Default for Emitter {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// String literals (cooked → Go quoting; raw → verbatim backticks)
// ---------------------------------------------------------------------------

pub(crate) fn go_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub mod expr;
pub mod stmt;
#[cfg(test)]
mod tests;
