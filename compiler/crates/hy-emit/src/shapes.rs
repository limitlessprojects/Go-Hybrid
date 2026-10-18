//! hy-emit/src/shapes.rs — the foreign shape table (Batch E: example 06).
//!
//! The MVP-0 binding hints (BindingKind) answer "is this binding foreign?"
//! for ONE member level. Example 06 needs three: `r.url.query.get("a")` is a
//! FIELD (`r.URL`), a METHOD GO SPELLS WITHOUT ARGS IN HY (`r.URL.Query()`),
//! and a METHOD (`Get`) — and the closure converting to `http.handler` needs
//! its untyped params typed by the TARGET (`http.HandlerFunc`'s signature).
//!
//! This table is the curated-surface answer (SPEC M5: bindings are
//! per-package work; promising "any Go package" would be the dishonest
//! version). It is deliberately small: exactly what the ten pinned programs
//! need, grown by Decision Log entries. A real type checker supersedes it
//! post-MVP (the table's keys ARE the descriptor types that checker will
//! resolve), so nothing here is throwaway spelling.
//!
//! Discipline: every entry names the example that pins it. Members nobody
//! uses are not here — "unsupported" is the clean error, never a guess.

/// How a member is spelled in Go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberKind {
    /// A Go field: `r.URL` — no parens, ever.
    Field,
    /// A Go method called with no arguments in this surface:
    /// hy spells `r.url.query`, Go requires `r.URL.Query()`.
    MethodZero,
    /// A Go method: args follow the call.
    Method,
}

/// One member of a foreign type.
#[derive(Debug, Clone, Copy)]
pub struct Member {
    /// Go spelling — EXACT (`"URL"`, not `to_go_name("url")` = `"Url"`).
    pub go: &'static str,
    pub kind: MemberKind,
    /// The descriptor type key of the member's result, for chain
    /// continuation (`url` on http.request → Some("url.url")).
    pub result: Option<&'static str>,
}

const fn field(go: &'static str, result: Option<&'static str>) -> Member {
    Member {
        go,
        kind: MemberKind::Field,
        result,
    }
}

const fn method0(go: &'static str, result: Option<&'static str>) -> Member {
    Member {
        go,
        kind: MemberKind::MethodZero,
        result,
    }
}

const fn method(go: &'static str, result: Option<&'static str>) -> Member {
    Member {
        go,
        kind: MemberKind::Method,
        result,
    }
}

/// A foreign type's member table, keyed by the hy type name
/// (`http.request` — dotted, snake_case, exactly as the surface spells it).
pub struct TypeShape {
    pub members: &'static [(&'static str, Member)],
}

/// The curated table. Keys are hy type names; every entry names its pinning
/// example. `http.header` rides along (response_writer.header's result) so
/// the chain continues if an example needs it.
pub static SHAPES: &[(&str, TypeShape)] = &[
    (
        // 06: r.method, r.url (fields on *http.Request)
        "http.request",
        TypeShape {
            members: &[
                ("method", field("Method", None)),
                ("url", field("URL", Some("url.url"))),
                ("header", field("Header", Some("http.header"))),
                ("host", field("Host", None)),
                ("remote_addr", field("RemoteAddr", None)),
                ("body", field("Body", None)),
            ],
        },
    ),
    (
        // 06: r.url.path (field), r.url.query (zero-arg METHOD — Go parses
        // RawQuery on every Query() call; the hy surface hides that).
        "url.url",
        TypeShape {
            members: &[
                ("path", field("Path", None)),
                ("raw_path", field("RawPath", None)),
                ("raw_query", field("RawQuery", None)),
                ("host", field("Host", None)),
                ("query", method0("Query", Some("url.values"))),
            ],
        },
    ),
    (
        // 06: r.url.query.get("a") — url.Values.Get (first value for a key).
        "url.values",
        TypeShape {
            members: &[
                ("get", method("Get", None)),
                ("set", method("Set", None)),
                ("has", method("Has", None)),
                ("encode", method("Encode", None)),
            ],
        },
    ),
    (
        // 06: w as http.ResponseWriter — header()/write/writeHeader.
        "http.response_writer",
        TypeShape {
            members: &[
                ("header", method0("Header", Some("http.header"))),
                ("write", method("Write", None)),
                ("write_header", method("WriteHeader", None)),
            ],
        },
    ),
    (
        // http.Header — the result of w.header(); get/set/add.
        "http.header",
        TypeShape {
            members: &[
                ("get", method("Get", None)),
                ("set", method("Set", None)),
                ("add", method("Add", None)),
            ],
        },
    ),
    (
        // 06: next.serve_http(w, r) — the http.Handler interface's method.
        "http.handler",
        TypeShape {
            members: &[("serve_http", method("ServeHTTP", None))],
        },
    ),
    (
        // 06: mux.handle(path, handler) / mux.handle_func.
        "http.serve_mux",
        TypeShape {
            members: &[
                ("handle", method("Handle", None)),
                ("handle_func", method("HandleFunc", None)),
                ("serve_http", method("ServeHTTP", None)),
            ],
        },
    ),
    (
        // 10: dt.milliseconds — hy spells Duration's methods property-style;
        // Go requires the call parens (MethodZero).
        "time.duration",
        TypeShape {
            members: &[
                ("milliseconds", method0("Milliseconds", None)),
                ("microseconds", method0("Microseconds", None)),
                ("nanoseconds", method0("Nanoseconds", None)),
                ("seconds", method0("Seconds", None)),
                ("minutes", method0("Minutes", None)),
                ("hours", method0("Hours", None)),
            ],
        },
    ),
    (
        // 10: t0 from time.now() — typed so since(t0) chains cleanly; no
        // members the pinned surface uses yet.
        "time.time",
        TypeShape { members: &[] },
    ),
    (
        // 05: methods on a serde_json handle value (all Methods — Go's
        // binding spells them as value-receiver calls).
        "serde_json.value",
        TypeShape {
            members: &[
                ("array", method("Array", None)),
                ("int", method("Int", None)),
                ("string", method("String", None)),
                ("set", method("Set", None)),
                ("push_string", method("PushString", None)),
            ],
        },
    ),
    (
        // 18: sync.Mutex — value methods on a user-struct field
        // (`stats.mu.lock()` → `stats.mu.Lock()`).
        "sync.mutex",
        TypeShape {
            members: &[
                ("lock", method("Lock", None)),
                ("unlock", method("Unlock", None)),
                ("try_lock", method("TryLock", None)),
            ],
        },
    ),
    (
        // sync.RWMutex rides along for the pool family (rlock/runlock).
        "sync.rwmutex",
        TypeShape {
            members: &[
                ("lock", method("Lock", None)),
                ("unlock", method("Unlock", None)),
                ("rlock", method("RLock", None)),
                ("runlock", method("RUnlock", None)),
            ],
        },
    ),
    (
        // 17: atomic.int64 — Go's atomic.Int64 STRUCT (methods Add/Load/
        // Store/Swap with pointer receivers; Go auto-addresses value
        // receivers, so both `var hits atomic.Int64` and `hits *atomic.
        // Int64` call `hits.Add(1)` identically). hy spells the methods
        // lowercase; the shape table carries the exact Go names.
        "atomic.int64",
        TypeShape {
            members: &[
                ("load", method("Load", None)),
                ("store", method("Store", None)),
                ("add", method("Add", None)),
                ("swap", method("Swap", None)),
            ],
        },
    ),
];

/// Result shapes for binding-value methods: (descriptor type, method) →
/// (n_results, has_error). The `?` hoist needs the arity BEFORE emitting
/// (`doc.set(k, v)?` has no value temp — Set returns only error). Methods
/// not listed default to (1, true) — the rust-call convention.
pub static METHOD_RESULTS: &[(&str, &str, (u8, bool))] = &[
    ("serde_json.value", "array", (1, true)),
    ("serde_json.value", "int", (1, true)),
    ("serde_json.value", "string", (1, true)),
    ("serde_json.value", "set", (0, true)),
    ("serde_json.value", "push_string", (0, true)),
];

pub fn lookup_method_results(ty: &str, name: &str) -> Option<(u8, bool)> {
    METHOD_RESULTS
        .iter()
        .find(|(t, m, _)| *t == ty && *m == name)
        .map(|(_, _, r)| *r)
}

/// Member lookup: (descriptor type, hy member name) → Member.
pub fn lookup_member(ty: &str, name: &str) -> Option<&'static Member> {
    let (_, shape) = SHAPES.iter().find(|(k, _)| *k == ty)?;
    let (_, m) = shape.members.iter().find(|(k, _)| *k == name)?;
    Some(m)
}

// ---------------------------------------------------------------------------
// Closure-conversion targets (E11's carrier): a hy type whose VALUE is
// really a Go func needing a named conversion at the conversion site.
// ---------------------------------------------------------------------------

/// `http.handler(fn(w, r) { … })` → `http.HandlerFunc(func(w http.
/// ResponseWriter, r *http.Request) { … })` — an untyped fn literal takes
/// its param types from the TARGET, and `?` inside it propagates through
/// http.Error (the E11 handler contract), because the target signature has
/// no error result for E3's return to land in.
pub struct Conversion {
    /// Go type text of the CONVERSION constructor (`http.HandlerFunc`).
    pub go: &'static str,
    /// The underlying func's params. `ty_key` is the shape-table descriptor
    /// (chains resolve through it); `go` is the EXACT Go type text; `pointer`
    /// distinguishes `w ResponseWriter` (interface, by value) from
    /// `r *Request` (pointer) — Go's HandlerFunc signature mixes both.
    pub params: &'static [ParamDesc],
    /// The E11 propagation contract, if this target has one:
    /// the hy member name of the "writer" first param used by the
    /// propagation statement (`http.Error(w, err, 400)`).
    pub propagate: Option<&'static str>,
}

pub struct ParamDesc {
    #[allow(dead_code)] // documents the surface; the emitter keys on position
    pub name: &'static str,
    pub ty_key: &'static str,
    pub go: &'static str,
    pub pointer: bool,
}

pub static CONVERSIONS: &[(&str, Conversion)] = &[(
    // Pinned by example 06. http.Handler is an INTERFACE — a bare func
    // literal does not implement it; Go's conversion type is HandlerFunc.
    "http.handler",
    Conversion {
        go: "http.HandlerFunc",
        params: &[
            ParamDesc {
                name: "w",
                ty_key: "http.response_writer",
                go: "http.ResponseWriter",
                pointer: false,
            },
            ParamDesc {
                name: "r",
                ty_key: "http.request",
                go: "http.Request",
                pointer: true,
            },
        ],
        propagate: Some("w"),
    },
)];

pub fn lookup_conversion(ty: &str) -> Option<&'static Conversion> {
    CONVERSIONS.iter().find(|(k, _)| *k == ty).map(|(_, c)| c)
}

// ---------------------------------------------------------------------------
// Rust crate bindings (M6): `import "rust/X"` → a generated-shape Go binding
// package, one descriptor per pinned crate. The emitter lowers
// `X.f(args…)` to `bindingPkg.GoName(args…)` with named args placed by the
// descriptor's param order (Go has no named args — example 10's `seed=7`).
// ---------------------------------------------------------------------------

pub struct RustFnDesc {
    /// hy surface name (`sha3.digest256`'s `digest256`).
    pub hy: &'static str,
    /// Go name in the binding package.
    pub go: &'static str,
    /// Param names in order — named args land here; positional args fill
    /// free slots left-to-right.
    pub params: &'static [&'static str],
    /// (n_results, has_error) — the Go wrapper's actual signature. The `?`
    /// hoist reads it: a 0-result call emits `hyErrN := call` (no value
    /// temp); a no-error call under `?` is a surface error.
    pub results: (u8, bool),
    /// Coerce string args to []byte at the call (`[]byte(arg)` is legal Go
    /// even when the arg is already a byte slice — a copy, which the
    /// copy-small/pin-large protocol allows).
    pub bytes_coerce: bool,
}

pub struct RustCrateDesc {
    /// hy package root (`sha3` in `import "rust/sha3"`).
    pub hy: &'static str,
    /// The binding Go package's ROOT (import path via import_path_of).
    pub binding_root: &'static str,
    pub fns: &'static [RustFnDesc],
}

pub static RUST_CRATES: &[RustCrateDesc] = &[
    // Pinned by example 10; the binding lives in runtime/go/sha3binding.
    RustCrateDesc {
        hy: "sha3",
        binding_root: "sha3binding",
        fns: &[
            RustFnDesc {
                hy: "digest256",
                go: "Digest256",
                params: &["data"],
                bytes_coerce: true,
                results: (1, true),
            },
            RustFnDesc {
                hy: "stream_checksum",
                go: "StreamChecksum",
                params: &["iters", "seed"],
                bytes_coerce: false,
                results: (1, true),
            },
        ],
    },
    // Pinned by example 05; the binding lives in runtime/go/jsonbinding.
    RustCrateDesc {
        hy: "serde_json",
        binding_root: "jsonbinding",
        fns: &[
            RustFnDesc {
                hy: "parse",
                go: "Parse",
                params: &["data"],
                bytes_coerce: true,
                results: (1, true),
            },
            RustFnDesc {
                hy: "new_object",
                go: "NewObject",
                params: &[],
                bytes_coerce: false,
                results: (1, false),
            },
            RustFnDesc {
                hy: "new_array",
                go: "NewArray",
                params: &[],
                bytes_coerce: false,
                results: (1, false),
            },
            RustFnDesc {
                hy: "to_string_pretty",
                go: "ToStringPretty",
                params: &["v"],
                bytes_coerce: false,
                results: (1, true),
            },
        ],
    },
    // Pinned by example 09 (the adoption story); binding in runtime/go/regexbinding.
    // NOTE bytes_coerce=false: the binding's Go API takes string args and
    // pins them itself (the copy-small protocol lives inside the binding).
    RustCrateDesc {
        hy: "regex",
        binding_root: "regexbinding",
        fns: &[
            RustFnDesc {
                hy: "is_match",
                go: "IsMatch",
                params: &["pattern", "text"],
                bytes_coerce: false,
                results: (1, true),
            },
            RustFnDesc {
                hy: "is_valid",
                go: "IsValid",
                params: &["pattern"],
                bytes_coerce: false,
                results: (1, false),
            },
        ],
    },
];

/// Rust-owned types cross as binding-package handles: `serde_json.value`
/// spells `jsonbinding.Value` in Go. Keyed by the hy type name.
pub static RUST_TYPES: &[(&str, &str, &str)] = &[
    // (hy type, binding root, Go type)
    ("serde_json.value", "jsonbinding", "Value"),
];

pub fn lookup_rust_type(hy_ty: &str) -> Option<(&'static str, &'static str)> {
    RUST_TYPES
        .iter()
        .find(|(k, _, _)| *k == hy_ty)
        .map(|(_, root, ty)| (*root, *ty))
}

pub fn lookup_rust_crate(root: &str) -> Option<&'static RustCrateDesc> {
    RUST_CRATES.iter().find(|c| c.hy == root)
}

/// Crates the CHECKER's foreign table knows (type shapes for the A7
/// consume-mode fixture and the fail-closed doctrine) but the RUNTIME does
/// not ship a binding for. A program using one cannot build or run — the
/// emitter refuses it with an honest diagnostic instead of the cryptic
/// `undefined: flate2` / `package flate2 is not in std` go-build failure
/// (the adoption-audit follow-up row). Shipping one = vendor the binding
/// package under runtime/go/<crate>binding, add its descriptor to
/// RUST_CRATES, and REMOVE it from this list — in the same commit.
pub static UNBOUND_RUST_CRATES: &[&str] = &["flate2"];

pub fn lookup_unbound_crate(root: &str) -> Option<&'static str> {
    UNBOUND_RUST_CRATES.iter().copied().find(|&c| c == root).map(|v| v as _)
}

// ---------------------------------------------------------------------------
// Import-path mapping (SPEC M5's curated allowlist, mechanical part).
// ---------------------------------------------------------------------------

/// hy package root → Go import path. Most roots are 1:1; net/* differ.
/// The emitted CODE always refers to the root spelling (`http.HandlerFunc`)
/// — Go's package name for `net/http` IS `http`, so the code is unchanged;
/// only the import line's path differs.
///
/// `hyb build` assembles every program as module `hybrid.program` with the
/// runtime packages copied inside — the binding imports are
/// `hybrid.program/<binding_root>`.
pub fn import_path_of(root: &str) -> String {
    match root {
        "http" => "net/http".to_string(),
        "httptest" => "net/http/httptest".to_string(),
        "hex" => "encoding/hex".to_string(),
        // 17: the code says `atomic.Int64` — Go's package name for the
        // path is `atomic`, but the PATH is sync/atomic (the naive last-
        // segment import `"atomic"` is not in std).
        "atomic" => "sync/atomic".to_string(),
        "sha3binding" => "hybrid.program/sha3binding".to_string(),
        "jsonbinding" => "hybrid.program/jsonbinding".to_string(),
        "regexbinding" => "hybrid.program/regexbinding".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_chain_resolves_three_levels() {
        // r.url.query.get — the exact example-06 chain.
        let url_m = lookup_member("http.request", "url").unwrap();
        assert_eq!(url_m.go, "URL");
        assert_eq!(url_m.kind, MemberKind::Field);
        assert_eq!(url_m.result, Some("url.url"));

        let q_m = lookup_member("url.url", "query").unwrap();
        assert_eq!(q_m.go, "Query");
        assert_eq!(q_m.kind, MemberKind::MethodZero);
        assert_eq!(q_m.result, Some("url.values"));

        let g_m = lookup_member("url.values", "get").unwrap();
        assert_eq!(g_m.go, "Get");
        assert_eq!(g_m.kind, MemberKind::Method);
    }

    #[test]
    fn unknown_members_are_none() {
        assert!(lookup_member("http.request", "nope").is_none());
        assert!(lookup_member("made.up.type", "url").is_none());
    }

    #[test]
    fn handler_conversion_shape() {
        let c = lookup_conversion("http.handler").unwrap();
        assert_eq!(c.go, "http.HandlerFunc");
        assert_eq!(c.params.len(), 2);
        assert_eq!(c.params[0].name, "w");
        assert_eq!(c.params[0].ty_key, "http.response_writer");
        assert!(!c.params[0].pointer, "the writer is an interface value");
        assert!(c.params[1].pointer, "the request is a pointer");
        assert_eq!(c.propagate, Some("w"));
    }

    #[test]
    fn import_paths_map_net_packages() {
        assert_eq!(import_path_of("http"), "net/http");
        assert_eq!(import_path_of("httptest"), "net/http/httptest");
        assert_eq!(import_path_of("fmt"), "fmt");
        assert_eq!(import_path_of("strconv"), "strconv");
    }
}

// ---------------------------------------------------------------------------
// SPEC primitive spellings (issue #147)
// ---------------------------------------------------------------------------

/// The SPEC-LANGUAGE fixed-width primitive names (SPEC-LANGUAGE §402, §2030)
/// in their Go spelling. `byte`/`rune` are already Go builtins and `int`/`uint`
/// are word-sized in both languages, so they are not listed — a name absent
/// here is emitted verbatim, exactly as before.
///
/// Deliberately NOT translated: `usize`/`isize`. SPEC-LANGUAGE §402 states
/// "There is no `usize`/`isize` at the language level" — they have no Go
/// equivalent, and inventing one (`u64`) would be a silent wrong-type answer.
/// Tone: failing to build is the honest outcome for a name the language does
/// not define.
pub fn go_primitive_name(name: &str) -> Option<&'static str> {
    Some(match name {
        // unsigned (SPEC spelling -> Go spelling)
        "u8" => "uint8",
        "u16" => "uint16",
        "u32" => "uint32",
        "u64" => "uint64",
        // signed
        "i8" => "int8",
        "i16" => "int16",
        "i32" => "int32",
        "i64" => "int64",
        // floats
        "f32" => "float32",
        "f64" => "float64",
        // Go spellings are the same Hybrid types (the checker's
        // fixed_width_int_range treats `int8` == `i8`, `byte` == `u8`),
        // so a conversion call written the Go way must take the SAME
        // conversion branch: identity-mapped it reaches the SPEC X4
        // constant fold instead of the generic call path, where
        // `int8(300)` used to pass through and die as
        // `constant 300 overflows int8` (issue #228 family C).
        "byte" => "byte",
        "rune" => "rune",
        "int8" => "int8",
        "int16" => "int16",
        "int32" => "int32",
        "uint8" => "uint8",
        "uint16" => "uint16",
        "uint32" => "uint32",
        _ => return None,
    })
}
