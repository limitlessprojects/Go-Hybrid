//! foreign.rs — the builtin foreign-type and method-mode tables.
//!
//! ── TECH DEBT (post-MVP, tracked for the lead) ────────────────────────────
//! These builtin descriptor STUBS stand in for the real boundary descriptors
//! (interfaces/abi-types.md, OQ8): the mode of every foreign call is
//! hardcoded here as data. When the bridge lands, this module is replaced by
//! generated descriptor tables (`own` / `borrow` / `mutate` / `consume`
//! flags per API, exactly as `finish` is flagged `consume` today). Until
//! then: unknown receivers degrade to `CallMode::Unknown` = "no loan
//! implications" (permissive — never a false reject; LOAN rules only, this
//! is not a safety pass), and unknown foreign TYPES degrade to
//! `TypeShape::Unknown` — which since O4.9 (rule FC, 2026-10-13) is
//! FAIL-CLOSED at spawn/send boundaries: a foreign result the stubs don't
//! describe cannot silently cross (HYB-E3005). Describing more of the real
//! boundary is exactly what the generated descriptor tables will widen.
//! ──────────────────────────────────────────────────────────────────────────

use crate::model::TypeShape;

/// Foreign package roots recognized without an import line (the corpus uses
/// `strings`/`fmt` freely; imports add to this set at runtime).
pub(crate) const BUILTIN_PACKAGES: &[&str] = &[
    "serde_json",
    "flate2",
    "atomic",
    "strings",
    "fmt",
    "http",
    "httptest",
    "sync",
    "net",
    "io",
    "os",
    "time",
    "sort",
    "strconv",
    "errors",
];

/// How a call uses its receiver / arguments (O7 handle modes + O13 consume).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CallMode {
    /// Creates a fresh own handle (does not touch any existing binding).
    Produce,
    /// Borrow-mode call: creates a read loan on the receiver (A2 window).
    Borrow,
    /// Mutating-mode call: conflicts with live borrow loans (A2).
    Mutate,
    /// Consuming call: own-transfer INTO the API — receiver moved-from (A7).
    Consume,
    /// Go-side / no loan semantics (fmt, strings, atomics, ...).
    Safe,
    /// Not in the table: no loan implications (permissive).
    Unknown,
}

/// The SCALAR `sync/atomic` types — the only atomics that are class 2
/// (sendable-by-type). Each one holds a plain machine value behind
/// internally synchronized Load/Store/Add/Swap/CompareAndSwap, so sharing
/// it across tasks cannot alias mutable GC state.
pub(crate) const SCALAR_ATOMICS: &[&str] = &[
    "atomic.bool",
    "atomic.int32",
    "atomic.int64",
    "atomic.uint32",
    "atomic.uint64",
    "atomic.uintptr",
];

/// Is `name` a scalar atomic (class 2)?
pub(crate) fn is_scalar_atomic(name: &str) -> bool {
    SCALAR_ATOMICS.contains(&name)
}

/// Is `name` a `sync/atomic` type OUTSIDE the scalar set — `atomic.pointer[T]`,
/// `atomic.value`, or anything else under `atomic.`? GUARANTEES.md row 11:
/// these are not on the curated surface yet ("unproven → rejected"). They
/// publish a reference to arbitrary (possibly mutable GC) state: the atomic
/// makes the pointer swap race-free, NOT the pointee, so treating them as
/// class 2 let two tasks write the same map through `p.load()`.
pub(crate) fn is_unproven_atomic(name: &str) -> bool {
    name.starts_with("atomic.") && !is_scalar_atomic(name)
}

/// Classify a dotted foreign type name; `None` = not a known foreign type
/// (the caller falls back to local struct decls / `Unknown`).
pub(crate) fn foreign_named_type(name: &str) -> Option<TypeShape> {
    if is_scalar_atomic(name) {
        return Some(TypeShape::Atomic);
    }
    if is_unproven_atomic(name) {
        // fail-closed (O4.9): Unknown → HYB-E3005 at spawn/send boundaries,
        // with the row-11 explanation (sendability.rs)
        return Some(TypeShape::Unknown);
    }
    match name {
        "serde_json.value" => Some(TypeShape::OwnHandle("serde_json.value".into())),
        "serde_json.view" => Some(TypeShape::BorrowHandle("serde_json.view".into())),
        "flate2.writer" => Some(TypeShape::OwnHandle("flate2.writer".into())),
        _ => {
            let root = name.split('.').next().unwrap_or("");
            if BUILTIN_PACKAGES.contains(&root) {
                Some(TypeShape::ForeignOpaque(name.to_string()))
            } else {
                None
            }
        }
    }
}

/// Mode of a package-level call: `serde_json.parse(...)`, `fmt.println(...)`.
pub(crate) fn package_mode(pkg: &str, method: &str) -> CallMode {
    match (pkg, method) {
        ("serde_json", "parse") | ("serde_json", "new_object") | ("flate2", "new_writer") => {
            CallMode::Produce
        }
        ("serde_json", "to_string_pretty")
        | ("serde_json", "array")
        | ("serde_json", "get")
        | ("serde_json", "int")
        | ("serde_json", "string") => CallMode::Borrow,
        ("serde_json", "set") | ("serde_json", "remove") => CallMode::Mutate,
        ("flate2", "write") => CallMode::Mutate,
        ("flate2", "finish") => CallMode::Consume,
        _ => CallMode::Safe,
    }
}

/// Result shape of a package-level call (for defines / loop variables).
pub(crate) fn package_result(pkg: &str, method: &str) -> TypeShape {
    match (pkg, method) {
        ("serde_json", "parse") | ("serde_json", "new_object") => {
            TypeShape::OwnHandle("serde_json.value".into())
        }
        ("flate2", "new_writer") => TypeShape::OwnHandle("flate2.writer".into()),
        ("serde_json", "array" | "get") => TypeShape::BorrowHandle("serde_json.view".into()),
        ("serde_json", "int") | ("flate2", "write") => TypeShape::Primitive,
        ("flate2", "finish") => TypeShape::Slice(Box::new(TypeShape::Primitive)),
        ("serde_json", "to_string_pretty") | ("serde_json", "string") | ("strings", "join") => {
            TypeShape::Str
        }
        ("strings", "split") | ("strings", "fields") => TypeShape::Slice(Box::new(TypeShape::Str)),
        ("http", "new_serve_mux") => TypeShape::ForeignOpaque("http.serve_mux".into()),
        ("http", "handler") => TypeShape::ForeignOpaque("http.handler".into()),
        // `atomic.load_pointer` / `atomic.swap_pointer` return an
        // unsafe.Pointer to arbitrary state — not a primitive (row 11)
        ("atomic", "load_pointer" | "swap_pointer") => TypeShape::Unknown,
        _ if pkg == "atomic" => TypeShape::Primitive,
        _ => TypeShape::Unknown,
    }
}

/// Mode of a method call on a typed receiver, keyed by the receiver's
/// boundary type name ("serde_json.value", "flate2.writer", "atomic", ...).
pub(crate) fn method_mode(recv_type: &str, method: &str) -> CallMode {
    if recv_type.starts_with("atomic.") {
        // atomics are safe-by-type: no loan implications (class 2)
        return CallMode::Safe;
    }
    match (recv_type, method) {
        (
            "serde_json.value" | "serde_json.view",
            "array" | "get" | "int" | "string" | "to_string_pretty",
        ) => CallMode::Borrow,
        ("serde_json.value" | "serde_json.view", "set" | "remove") => CallMode::Mutate,
        ("flate2.writer", "write") => CallMode::Mutate,
        ("flate2.writer", "finish") => CallMode::Consume,
        _ => CallMode::Unknown,
    }
}

/// Result shape of a method call on a typed receiver.
pub(crate) fn method_result(recv_type: &str, method: &str) -> TypeShape {
    match (recv_type, method) {
        ("serde_json.value" | "serde_json.view", "array" | "get") => {
            // element views: iterating the result yields borrow handles
            TypeShape::BorrowHandle("serde_json.view".into())
        }
        ("serde_json.value" | "serde_json.view", "int") => TypeShape::Primitive,
        ("serde_json.value" | "serde_json.view", "string" | "to_string_pretty") => TypeShape::Str,
        ("flate2.writer", "write") => TypeShape::Primitive,
        ("flate2.writer", "finish") => TypeShape::Slice(Box::new(TypeShape::Primitive)),
        // scalar atomics only ("atomic.int" is receiver_type_name's key for
        // TypeShape::Atomic): load/swap/add yield the scalar. A non-scalar
        // atomic's `load()` / `swap()` returns a pointer / `any` — Unknown,
        // fail-closed (GUARANTEES row 11).
        _ if recv_type == "atomic.int" || is_scalar_atomic(recv_type) => TypeShape::Primitive,
        _ => TypeShape::Unknown,
    }
}
