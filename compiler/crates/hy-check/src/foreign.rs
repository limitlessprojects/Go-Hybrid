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

/// The foreign handles the emitter ranges as element sources — the
/// `ForeignTy` range branch in `hy-emit`'s statement emission, keyed by the
/// same descriptors as the emitter's SHAPES table. `serde_json.value` is the
/// element-source handle (corpus 05 ranges `doc.array(...)` views and the
/// emitter supports ranging the value handle itself); other handles (e.g.
/// `flate2.writer`) are NOT iterable and must stay rejected (issue #411).
pub(crate) fn is_iterable_handle(name: &str) -> bool {
    name == "serde_json.value"
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

/// The argument index a package-level call mutates IN PLACE (T22: passing a
/// `const` root there is a write, `HYB-E3006`). `None` = the descriptor stubs
/// record no in-place write for this API.
pub(crate) fn package_mutated_arg(pkg: &str, method: &str) -> Option<usize> {
    match (pkg, method) {
        (
            "sort",
            "ints" | "strings" | "float64s" | "slice" | "slice_stable" | "sort" | "stable"
            | "Ints" | "Strings" | "Float64s" | "Slice" | "SliceStable" | "Sort" | "Stable",
        ) => Some(0),
        (
            "io",
            "read_full" | "read_at_least" | "ReadFull" | "ReadAtLeast",
        ) => Some(1),
        ("io", "copy_buffer" | "CopyBuffer") => Some(2),
        _ => None,
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
        // `time.after(d)` returns a receive-only channel (the deadline
        // pattern, example 04); the element shape is opaque here — the
        // binding still resolves as a channel so receives on it classify.
        ("time", "after") => TypeShape::Chan(Box::new(TypeShape::Unknown)),
        ("http", "new_serve_mux") => TypeShape::ForeignOpaque("http.serve_mux".into()),
        ("http", "handler") => TypeShape::ForeignOpaque("http.handler".into()),
        // `atomic.load_pointer` / `atomic.swap_pointer` return an
        // unsafe.Pointer to arbitrary state — not a primitive (row 11)
        ("atomic", "load_pointer" | "swap_pointer") => TypeShape::Unknown,
        ("errors", "new" | "wrap" | "New" | "Wrap") => TypeShape::ForeignOpaque("error".into()),
        ("errors", "is" | "Is") => TypeShape::Primitive,
        _ if pkg == "atomic" => TypeShape::Primitive,
        _ => TypeShape::Unknown,
    }
}

/// Static result count of a foreign package-level call returning multiple
/// values in Go (`(T, error)`, `(before, after, found)`, etc.) when not
/// unwrapped with `?` (#380 / T23 `HYB-E2002`).
pub(crate) fn package_result_count(pkg: &str, method: &str) -> Option<usize> {
    match (pkg, method) {
        (
            "strconv",
            "parse_int"
                | "parse_uint"
                | "parse_float"
                | "parse_bool"
                | "parse_complex"
                | "atoi"
                | "unquote"
                | "ParseInt"
                | "ParseUint"
                | "ParseFloat"
                | "ParseBool"
                | "ParseComplex"
                | "Atoi"
                | "Unquote",
        ) => Some(2),
        (
            "os",
            "open"
                | "create"
                | "read_file"
                | "read_dir"
                | "getwd"
                | "user_home_dir"
                | "user_cache_dir"
                | "user_config_dir"
                | "executable"
                | "lookup_env"
                | "stat"
                | "lstat"
                | "create_temp"
                | "mkdir_temp"
                | "Open"
                | "Create"
                | "ReadFile"
                | "ReadDir"
                | "Getwd"
                | "UserHomeDir"
                | "UserCacheDir"
                | "UserConfigDir"
                | "Executable"
                | "LookupEnv"
                | "Stat"
                | "Lstat"
                | "CreateTemp"
                | "MkdirTemp",
        ) => Some(2),
        (
            "io",
            "read_all"
                | "read_full"
                | "read_at_least"
                | "copy"
                | "copy_n"
                | "copy_buffer"
                | "write_string"
                | "ReadAll"
                | "ReadFull"
                | "ReadAtLeast"
                | "Copy"
                | "CopyN"
                | "CopyBuffer"
                | "WriteString",
        ) => Some(2),
        (
            "strings",
            "cut_prefix" | "cut_suffix" | "CutPrefix" | "CutSuffix",
        ) => Some(2),
        ("strings", "cut" | "Cut") => Some(3),
        (
            "filepath",
            "abs"
                | "rel"
                | "eval_symlinks"
                | "glob"
                | "match"
                | "split"
                | "Abs"
                | "Rel"
                | "EvalSymlinks"
                | "Glob"
                | "Match"
                | "Split",
        ) => Some(2),
        ("path", "match" | "split" | "Match" | "Split") => Some(2),
        (
            "time",
            "parse"
                | "parse_duration"
                | "load_location"
                | "Parse"
                | "ParseDuration"
                | "LoadLocation",
        ) => Some(2),
        (
            "net",
            "listen"
                | "dial"
                | "resolve_tcp_addr"
                | "lookup_host"
                | "Listen"
                | "Dial"
                | "ResolveTCPAddr"
                | "LookupHost",
        ) => Some(2),
        ("net", "split_host_port" | "SplitHostPort") => Some(3),
        ("hex", "decode_string" | "DecodeString") => Some(2),
        ("regex", "compile" | "match_string" | "Compile" | "MatchString") => Some(2),
        _ => None,
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

/// Human-readable M5 curated package allowlist string for HYB-E1001 diagnostics (#658).
pub(crate) const CURATED_ALLOWLIST_DISPLAY: &str = "fmt, os, io, time, sort, strings, strconv, errors, sync, sync/atomic, bufio, net, net/http, net/http/httptest, net/url, encoding/json, encoding/hex, path, path/filepath, rust/serde_json, rust/sha3, rust/regex";

/// Is `path` in SPEC M5's curated Go allowlist or M6's pinned Rust crate set (#658)?
pub(crate) fn is_curated_import(path: &str) -> bool {
    matches!(
        path,
        "fmt"
            | "os"
            | "io"
            | "time"
            | "sort"
            | "strings"
            | "strconv"
            | "errors"
            | "sync"
            | "sync/atomic"
            | "atomic"
            | "bufio"
            | "net"
            | "net/http"
            | "http"
            | "net/http/httptest"
            | "httptest"
            | "net/url"
            | "url"
            | "encoding/json"
            | "json"
            | "encoding/hex"
            | "hex"
            | "path"
            | "path/filepath"
            | "filepath"
            | "rust/serde_json"
            | "serde_json"
            | "rust/sha3"
            | "sha3"
            | "rust/regex"
            | "regex"
            | "rust/flate2"
            | "flate2"
    )
}

/// Is `root` a Go standard library root outside the M5 curated allowlist (#658)?
pub(crate) fn is_uncurated_go_stdlib_root(root: &str) -> bool {
    matches!(
        root,
        "math"
            | "reflect"
            | "syscall"
            | "unsafe"
            | "runtime"
            | "log"
            | "flag"
            | "testing"
            | "unicode"
            | "bytes"
            | "context"
            | "regexp"
            | "crypto"
            | "sha256"
            | "sha512"
            | "rand"
            | "binary"
            | "base64"
            | "csv"
            | "xml"
            | "html"
            | "sql"
            | "exec"
            | "signal"
            | "user"
            | "gzip"
            | "zlib"
            | "tar"
            | "zip"
    )
}

/// Is `root` a curated package root whose members are resolved by M4/M5 (#658)?
pub(crate) fn is_curated_package_root(root: &str) -> bool {
    matches!(
        root,
        "fmt"
            | "os"
            | "io"
            | "time"
            | "sort"
            | "strings"
            | "strconv"
            | "errors"
            | "sync"
            | "atomic"
            | "bufio"
            | "net"
            | "http"
            | "httptest"
            | "url"
            | "json"
            | "hex"
            | "path"
            | "filepath"
            | "serde_json"
            | "sha3"
            | "regex"
            | "flate2"
    )
}

fn normalize_go_ident_to_snake(name: &str) -> String {
    if name.chars().all(|c| !c.is_ascii_uppercase()) {
        return name.to_string();
    }
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::new();
    for i in 0..chars.len() {
        let c = chars[i];
        if c.is_ascii_uppercase() {
            if i > 0 {
                let prev = chars[i - 1];
                let next_is_lower = chars.get(i + 1).is_some_and(|n| n.is_ascii_lowercase());
                if prev.is_ascii_lowercase()
                    || prev.is_ascii_digit()
                    || (prev.is_ascii_uppercase() && next_is_lower)
                {
                    out.push('_');
                }
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Type members of the curated Go packages, as distinct from exported values.
/// Keep this narrower than `foreign_named_type`: that shape fallback also
/// accepts unknown members and therefore cannot prove that a name is a type.
/// M4 snake_case and Go spellings share the member-resolution normalizer.
pub(crate) fn is_curated_package_type(pkg: &str, member: &str) -> bool {
    let norm = normalize_go_ident_to_snake(crate::model::Checker::base_named_type_str(member));
    match pkg {
        "fmt" => matches!(
            norm.as_str(),
            "stringer" | "go_stringer" | "state" | "formatter" | "scanner" | "scan_state"
        ),
        "os" => matches!(
            norm.as_str(),
            "file"
                | "file_info"
                | "file_mode"
                | "dir_entry"
                | "path_error"
                | "link_error"
                | "syscall_error"
                | "process"
                | "process_state"
                | "proc_attr"
                | "signal"
                | "root"
        ),
        "io" => matches!(
            norm.as_str(),
            "reader"
                | "writer"
                | "closer"
                | "read_writer"
                | "read_closer"
                | "write_closer"
                | "read_write_closer"
                | "read_seeker"
                | "read_seek_closer"
                | "write_seeker"
                | "read_write_seeker"
                | "reader_from"
                | "writer_to"
                | "reader_at"
                | "writer_at"
                | "byte_reader"
                | "byte_scanner"
                | "byte_writer"
                | "rune_reader"
                | "rune_scanner"
                | "string_writer"
                | "limited_reader"
                | "section_reader"
                | "offset_writer"
                | "pipe_reader"
                | "pipe_writer"
                | "seeker"
        ),
        "time" => matches!(
            norm.as_str(),
            "time"
                | "duration"
                | "timer"
                | "ticker"
                | "location"
                | "month"
                | "weekday"
                | "parse_error"
        ),
        "sort" => matches!(
            norm.as_str(),
            "interface" | "int_slice" | "float64_slice" | "string_slice"
        ),
        "strings" => matches!(norm.as_str(), "builder" | "reader" | "replacer"),
        "strconv" => norm == "num_error",
        "sync" => matches!(
            norm.as_str(),
            "mutex"
                | "rw_mutex"
                | "rwmutex"
                | "wait_group"
                | "once"
                | "cond"
                | "map"
                | "pool"
                | "locker"
        ),
        "atomic" => matches!(
            norm.as_str(),
            "bool" | "int32" | "int64" | "uint32" | "uint64" | "uintptr" | "pointer" | "value"
        ),
        "bufio" => matches!(
            norm.as_str(),
            "reader" | "writer" | "read_writer" | "scanner" | "split_func"
        ),
        "http" => matches!(
            norm.as_str(),
            "handler"
                | "handler_func"
                | "request"
                | "response"
                | "response_writer"
                | "header"
                | "serve_mux"
                | "server"
                | "client"
                | "transport"
                | "cookie"
                | "cookie_jar"
                | "round_tripper"
                | "flusher"
                | "hijacker"
                | "close_notifier"
                | "pusher"
                | "conn_state"
                | "dir"
                | "file"
                | "file_system"
                | "http2_config"
                | "max_bytes_error"
                | "protocol_error"
                | "protocols"
                | "response_controller"
                | "same_site"
        ),
        "httptest" => matches!(norm.as_str(), "response_recorder" | "server"),
        "net" => matches!(
            norm.as_str(),
            "conn"
                | "listener"
                | "packet_conn"
                | "addr"
                | "dialer"
                | "listen_config"
                | "resolver"
                | "ip"
                | "ip_mask"
                | "ip_net"
                | "ip_addr"
                | "ip_conn"
                | "tcp_addr"
                | "tcp_conn"
                | "tcp_listener"
                | "udp_addr"
                | "udp_conn"
                | "unix_addr"
                | "unix_conn"
                | "unix_listener"
                | "interface"
                | "hardware_addr"
                | "buffers"
                | "error"
                | "addr_error"
                | "dns_config_error"
                | "dns_error"
                | "invalid_addr_error"
                | "op_error"
                | "parse_error"
                | "unknown_network_error"
                | "flags"
                | "mx"
                | "ns"
                | "srv"
        ),
        "url" => matches!(
            norm.as_str(),
            "url" | "userinfo" | "values" | "error" | "escape_error" | "invalid_host_error"
        ),
        "json" => matches!(
            norm.as_str(),
            "decoder"
                | "encoder"
                | "raw_message"
                | "number"
                | "delim"
                | "token"
                | "marshaler"
                | "unmarshaler"
                | "invalid_unmarshal_error"
                | "marshaler_error"
                | "syntax_error"
                | "unmarshal_field_error"
                | "unmarshal_type_error"
                | "unsupported_type_error"
                | "unsupported_value_error"
        ),
        "hex" => norm == "invalid_byte_error",
        "filepath" => norm == "walk_func",
        _ => false,
    }
}

/// Does curated package `pkg` export `member` (under M4 snake_case or Go spelling, #658)?
pub(crate) fn is_curated_package_member(pkg: &str, member: &str) -> bool {
    let norm = normalize_go_ident_to_snake(member);
    let m = norm.as_str();
    match pkg {
        "fmt" => matches!(
            m,
            "print"
                | "println"
                | "printf"
                | "sprint"
                | "sprintln"
                | "sprintf"
                | "fprint"
                | "fprintln"
                | "fprintf"
                | "eprint"
                | "eprintln"
                | "eprintf"
                | "errorf"
                | "scan"
                | "scanln"
                | "scanf"
                | "sscan"
                | "sscanln"
                | "sscanf"
                | "fscan"
                | "fscanln"
                | "fscanf"
                | "append"
                | "appendf"
                | "appendln"
                | "format_string"
                | "stringer"
                | "go_stringer"
                | "state"
                | "formatter"
                | "scanner"
                | "scan_state"
        ),
        "os" => matches!(
            m,
            "args"
                | "stdout"
                | "stderr"
                | "stdin"
                | "open"
                | "open_file"
                | "create"
                | "create_temp"
                | "mkdir"
                | "mkdir_all"
                | "mkdir_temp"
                | "remove"
                | "remove_all"
                | "rename"
                | "read_file"
                | "write_file"
                | "read_dir"
                | "stat"
                | "lstat"
                | "chmod"
                | "chown"
                | "lchown"
                | "chtimes"
                | "chdir"
                | "getwd"
                | "getenv"
                | "lookup_env"
                | "setenv"
                | "unsetenv"
                | "clearenv"
                | "environ"
                | "expand"
                | "expand_env"
                | "exit"
                | "executable"
                | "hostname"
                | "getpid"
                | "getppid"
                | "getuid"
                | "geteuid"
                | "getgid"
                | "getegid"
                | "getgroups"
                | "getpagesize"
                | "same_file"
                | "is_exist"
                | "is_not_exist"
                | "is_permission"
                | "is_timeout"
                | "is_path_separator"
                | "link"
                | "symlink"
                | "readlink"
                | "temp_dir"
                | "user_cache_dir"
                | "user_config_dir"
                | "user_home_dir"
                | "pipe"
                | "new_file"
                | "find_process"
                | "start_process"
                | "copy_fs"
                | "dir_fs"
                | "truncate"
                | "file"
                | "file_info"
                | "file_mode"
                | "dir_entry"
                | "path_error"
                | "link_error"
                | "syscall_error"
                | "process"
                | "process_state"
                | "proc_attr"
                | "signal"
                | "interrupt"
                | "kill"
                | "dev_null"
                | "path_separator"
                | "path_list_separator"
                | "o_rdonly"
                | "o_wronly"
                | "o_rdwr"
                | "o_append"
                | "o_create"
                | "o_excl"
                | "o_sync"
                | "o_trunc"
                | "mode_dir"
                | "mode_append"
                | "mode_exclusive"
                | "mode_temporary"
                | "mode_symlink"
                | "mode_device"
                | "mode_named_pipe"
                | "mode_socket"
                | "mode_setuid"
                | "mode_setgid"
                | "mode_char_device"
                | "mode_sticky"
                | "mode_irregular"
                | "mode_type"
                | "mode_perm"
                | "err_invalid"
                | "err_permission"
                | "err_exist"
                | "err_not_exist"
                | "err_closed"
                | "err_no_deadline"
                | "err_deadline_exceeded"
                | "seek_set"
                | "seek_cur"
                | "seek_end"
        ),
        "io" => matches!(
            m,
            "eof"
                | "err_closed_pipe"
                | "err_no_progress"
                | "err_short_buffer"
                | "err_short_write"
                | "err_unexpected_eof"
                | "discard"
                | "read_all"
                | "read_at_least"
                | "read_full"
                | "write_string"
                | "copy"
                | "copy_buffer"
                | "copy_n"
                | "limit_reader"
                | "multi_reader"
                | "multi_writer"
                | "new_section_reader"
                | "nop_closer"
                | "pipe"
                | "tee_reader"
                | "reader"
                | "writer"
                | "closer"
                | "read_writer"
                | "read_closer"
                | "write_closer"
                | "read_write_closer"
                | "read_seeker"
                | "read_seek_closer"
                | "write_seeker"
                | "read_write_seeker"
                | "reader_from"
                | "writer_to"
                | "reader_at"
                | "writer_at"
                | "byte_reader"
                | "byte_scanner"
                | "byte_writer"
                | "rune_reader"
                | "rune_scanner"
                | "string_writer"
                | "limited_reader"
                | "section_reader"
                | "offset_writer"
                | "pipe_reader"
                | "pipe_writer"
                | "seeker"
                | "seek_start"
                | "seek_current"
                | "seek_end"
        ),
        "time" => matches!(
            m,
            "milliseconds"
                | "seconds"
                | "minutes"
                | "hours"
                | "nanoseconds"
                | "microseconds"
                | "now"
                | "since"
                | "until"
                | "sleep"
                | "after"
                | "after_func"
                | "tick"
                | "new_ticker"
                | "new_timer"
                | "date"
                | "unix"
                | "unix_milli"
                | "unix_micro"
                | "parse"
                | "parse_in_location"
                | "parse_duration"
                | "load_location"
                | "load_location_from_tz_data"
                | "fixed_zone"
                | "time"
                | "duration"
                | "timer"
                | "ticker"
                | "location"
                | "month"
                | "weekday"
                | "parse_error"
                | "nanosecond"
                | "microsecond"
                | "millisecond"
                | "second"
                | "minute"
                | "hour"
                | "layout"
                | "ansic"
                | "unix_date"
                | "ruby_date"
                | "rfc822"
                | "rfc822z"
                | "rfc850"
                | "rfc1123"
                | "rfc1123z"
                | "rfc3339"
                | "rfc3339_nano"
                | "kitchen"
                | "stamp"
                | "stamp_milli"
                | "stamp_micro"
                | "stamp_nano"
                | "date_time"
                | "date_only"
                | "time_only"
                | "january"
                | "february"
                | "march"
                | "april"
                | "may"
                | "june"
                | "july"
                | "august"
                | "september"
                | "october"
                | "november"
                | "december"
                | "sunday"
                | "monday"
                | "tuesday"
                | "wednesday"
                | "thursday"
                | "friday"
                | "saturday"
                | "utc"
                | "local"
        ),
        "sort" => matches!(
            m,
            "ints"
                | "ints_are_sorted"
                | "float64s"
                | "float64s_are_sorted"
                | "strings"
                | "strings_are_sorted"
                | "search"
                | "search_ints"
                | "search_float64s"
                | "search_strings"
                | "slice"
                | "slice_is_sorted"
                | "slice_stable"
                | "sort"
                | "stable"
                | "is_sorted"
                | "reverse"
                | "find"
                | "interface"
                | "int_slice"
                | "float64_slice"
                | "string_slice"
        ),
        "strings" => matches!(
            m,
            "clone"
                | "compare"
                | "contains"
                | "contains_any"
                | "contains_func"
                | "contains_rune"
                | "count"
                | "cut"
                | "cut_prefix"
                | "cut_suffix"
                | "equal_fold"
                | "fields"
                | "fields_func"
                | "has_prefix"
                | "has_suffix"
                | "index"
                | "index_any"
                | "index_byte"
                | "index_func"
                | "index_rune"
                | "join"
                | "last_index"
                | "last_index_any"
                | "last_index_byte"
                | "last_index_func"
                | "map"
                | "new_reader"
                | "new_replacer"
                | "repeat"
                | "replace"
                | "replace_all"
                | "split"
                | "split_after"
                | "split_after_n"
                | "split_n"
                | "title"
                | "to_lower"
                | "to_lower_special"
                | "to_title"
                | "to_title_special"
                | "to_upper"
                | "to_upper_special"
                | "to_valid_utf8"
                | "trim"
                | "trim_func"
                | "trim_left"
                | "trim_left_func"
                | "trim_prefix"
                | "trim_right"
                | "trim_right_func"
                | "trim_space"
                | "trim_suffix"
                | "builder"
                | "reader"
                | "replacer"
        ),
        "strconv" => matches!(
            m,
            "atoi"
                | "itoa"
                | "parse_bool"
                | "parse_complex"
                | "parse_float"
                | "parse_int"
                | "parse_uint"
                | "format_bool"
                | "format_complex"
                | "format_float"
                | "format_int"
                | "format_uint"
                | "append_bool"
                | "append_float"
                | "append_int"
                | "append_quote"
                | "append_quote_rune"
                | "append_quote_rune_to_ascii"
                | "append_quote_rune_to_graphic"
                | "append_quote_to_ascii"
                | "append_quote_to_graphic"
                | "append_uint"
                | "can_backquote"
                | "is_graphic"
                | "is_print"
                | "quote"
                | "quote_rune"
                | "quote_rune_to_ascii"
                | "quote_rune_to_graphic"
                | "quote_to_ascii"
                | "quote_to_graphic"
                | "quoted_prefix"
                | "unquote"
                | "unquote_char"
                | "int_size"
                | "err_range"
                | "err_syntax"
                | "num_error"
        ),
        "errors" => matches!(
            m,
            "new" | "wrap" | "is" | "as" | "join" | "unwrap" | "err_unsupported"
        ),
        "sync" => matches!(
            m,
            "mutex"
                | "rw_mutex"
                | "rwmutex"
                | "wait_group"
                | "once"
                | "cond"
                | "map"
                | "pool"
                | "locker"
                | "new_cond"
                | "once_func"
                | "once_value"
                | "once_values"
        ),
        "atomic" => matches!(
            m,
            "bool"
                | "int32"
                | "int64"
                | "uint32"
                | "uint64"
                | "uintptr"
                | "pointer"
                | "value"
                | "add_int32"
                | "add_int64"
                | "add_uint32"
                | "add_uint64"
                | "add_uintptr"
                | "compare_and_swap_int32"
                | "compare_and_swap_int64"
                | "compare_and_swap_uint32"
                | "compare_and_swap_uint64"
                | "compare_and_swap_uintptr"
                | "compare_and_swap_pointer"
                | "load_int32"
                | "load_int64"
                | "load_uint32"
                | "load_uint64"
                | "load_uintptr"
                | "load_pointer"
                | "store_int32"
                | "store_int64"
                | "store_uint32"
                | "store_uint64"
                | "store_uintptr"
                | "store_pointer"
                | "swap_int32"
                | "swap_int64"
                | "swap_uint32"
                | "swap_uint64"
                | "swap_uintptr"
                | "swap_pointer"
        ),
        "bufio" => matches!(
            m,
            "new_reader"
                | "new_reader_size"
                | "new_writer"
                | "new_writer_size"
                | "new_read_writer"
                | "new_scanner"
                | "scan_bytes"
                | "scan_lines"
                | "scan_runes"
                | "scan_words"
                | "reader"
                | "writer"
                | "read_writer"
                | "scanner"
                | "split_func"
                | "max_scan_token_size"
                | "err_advance_too_far"
                | "err_bad_read_count"
                | "err_buffer_full"
                | "err_final_token"
                | "err_invalid_unread_byte"
                | "err_invalid_unread_rune"
                | "err_negative_advance"
                | "err_negative_count"
                | "err_too_long"
        ),
        "http" => {
            m.starts_with("status_")
                || m.starts_with("method_")
                || m.starts_with("err_")
                || matches!(
                    m,
                    "new_serve_mux"
                        | "handle"
                        | "handle_func"
                        | "handler"
                        | "handler_func"
                        | "listen_and_serve"
                        | "listen_and_serve_tls"
                        | "serve"
                        | "serve_tls"
                        | "get"
                        | "post"
                        | "post_form"
                        | "head"
                        | "new_request"
                        | "new_request_with_context"
                        | "error"
                        | "not_found"
                        | "not_found_handler"
                        | "redirect"
                        | "redirect_handler"
                        | "strip_prefix"
                        | "timeout_handler"
                        | "file_server"
                        | "file_server_fs"
                        | "serve_content"
                        | "serve_file"
                        | "serve_file_fs"
                        | "canonical_header_key"
                        | "detect_content_type"
                        | "max_bytes_handler"
                        | "max_bytes_reader"
                        | "parse_http_version"
                        | "parse_time"
                        | "proxy_from_environment"
                        | "proxy_url"
                        | "read_request"
                        | "read_response"
                        | "set_cookie"
                        | "allow_query_semicolons"
                        | "request"
                        | "response"
                        | "response_writer"
                        | "header"
                        | "serve_mux"
                        | "server"
                        | "client"
                        | "transport"
                        | "cookie"
                        | "cookie_jar"
                        | "round_tripper"
                        | "flusher"
                        | "hijacker"
                        | "close_notifier"
                        | "pusher"
                        | "conn_state"
                        | "dir"
                        | "file"
                        | "file_system"
                        | "http2_config"
                        | "max_bytes_error"
                        | "protocol_error"
                        | "protocols"
                        | "response_controller"
                        | "same_site"
                        | "default_client"
                        | "default_serve_mux"
                        | "default_transport"
                        | "no_body"
                        | "time_format"
                        | "trailer_prefix"
                )
        }
        "httptest" => matches!(
            m,
            "get"
                | "post"
                | "new_recorder"
                | "new_request"
                | "new_server"
                | "new_tls_server"
                | "new_unstarted_server"
                | "response_recorder"
                | "server"
                | "default_remote_addr"
        ),
        "net" => matches!(
            m,
            "listen"
                | "listen_packet"
                | "listen_tcp"
                | "listen_udp"
                | "listen_ip"
                | "listen_unix"
                | "listen_unixgram"
                | "dial"
                | "dial_timeout"
                | "dial_tcp"
                | "dial_udp"
                | "dial_ip"
                | "dial_unix"
                | "split_host_port"
                | "join_host_port"
                | "lookup_addr"
                | "lookup_cname"
                | "lookup_host"
                | "lookup_ip"
                | "lookup_mx"
                | "lookup_ns"
                | "lookup_port"
                | "lookup_srv"
                | "lookup_txt"
                | "parse_cidr"
                | "parse_ip"
                | "parse_mac"
                | "interface_addrs"
                | "interface_by_index"
                | "interface_by_name"
                | "interfaces"
                | "ipv4"
                | "ipv4_mask"
                | "cidr_mask"
                | "pipe"
                | "resolve_ip_addr"
                | "resolve_tcp_addr"
                | "resolve_udp_addr"
                | "resolve_unix_addr"
                | "file_conn"
                | "file_listener"
                | "file_packet_conn"
                | "conn"
                | "listener"
                | "packet_conn"
                | "addr"
                | "dialer"
                | "listen_config"
                | "resolver"
                | "ip"
                | "ip_mask"
                | "ip_net"
                | "ip_addr"
                | "ip_conn"
                | "tcp_addr"
                | "tcp_conn"
                | "tcp_listener"
                | "udp_addr"
                | "udp_conn"
                | "unix_addr"
                | "unix_conn"
                | "unix_listener"
                | "interface"
                | "hardware_addr"
                | "buffers"
                | "error"
                | "addr_error"
                | "dns_config_error"
                | "dns_error"
                | "invalid_addr_error"
                | "op_error"
                | "parse_error"
                | "unknown_network_error"
                | "err_closed"
                | "err_write_to_connected"
                | "ipv4len"
                | "ipv6len"
                | "default_resolver"
        ),
        "url" => matches!(
            m,
            "parse"
                | "parse_request_uri"
                | "parse_query"
                | "query_escape"
                | "query_unescape"
                | "path_escape"
                | "path_unescape"
                | "join_path"
                | "user"
                | "user_password"
                | "url"
                | "values"
                | "userinfo"
                | "error"
                | "escape_error"
                | "invalid_host_error"
        ),
        "json" => matches!(
            m,
            "marshal"
                | "marshal_indent"
                | "unmarshal"
                | "new_decoder"
                | "new_encoder"
                | "compact"
                | "html_escape"
                | "indent"
                | "valid"
                | "decoder"
                | "encoder"
                | "raw_message"
                | "delim"
                | "number"
                | "marshaler"
                | "unmarshaler"
                | "syntax_error"
                | "unmarshal_type_error"
                | "unsupported_type_error"
                | "unsupported_value_error"
                | "marshaler_error"
        ),
        "hex" => matches!(
            m,
            "encode"
                | "encode_to_string"
                | "decode"
                | "decode_string"
                | "encoded_len"
                | "decoded_len"
                | "dump"
                | "dumper"
                | "new_encoder"
                | "new_decoder"
                | "err_length"
                | "invalid_byte_error"
        ),
        "path" => matches!(
            m,
            "base"
                | "clean"
                | "dir"
                | "ext"
                | "is_abs"
                | "join"
                | "match"
                | "split"
                | "err_bad_pattern"
        ),
        "filepath" => matches!(
            m,
            "abs"
                | "base"
                | "clean"
                | "dir"
                | "eval_symlinks"
                | "ext"
                | "from_slash"
                | "glob"
                | "has_prefix"
                | "is_abs"
                | "is_local"
                | "join"
                | "localize"
                | "match"
                | "rel"
                | "split"
                | "split_list"
                | "to_slash"
                | "volume_name"
                | "walk"
                | "walk_dir"
                | "walk_func"
                | "separator"
                | "list_separator"
                | "err_bad_pattern"
                | "skip_all"
                | "skip_dir"
        ),
        "serde_json" => matches!(
            m,
            "parse"
                | "new_object"
                | "new_array"
                | "to_string_pretty"
                | "array"
                | "get"
                | "int"
                | "string"
                | "set"
                | "remove"
                | "push_string"
                | "value"
                | "view"
        ),
        "sha3" => matches!(m, "digest256" | "stream_checksum"),
        "regex" => matches!(m, "is_match" | "is_valid" | "compile" | "match_string"),
        "flate2" => matches!(m, "new_writer" | "write" | "finish" | "writer"),
        _ => true,
    }
}

