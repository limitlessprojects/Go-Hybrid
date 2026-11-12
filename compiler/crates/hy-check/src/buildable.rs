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
    AssignOp, BinOp, Block, CallArg, Decl, Expr, FieldInit, File, FnDecl, FnResult, ForKind,
    InterpPart, LitKind, LocalDecl, LocalKind, MatchPattern, Param, Place, PlaceKind, SelectArmKind,
    Stmt, TypeExpr, UnOp,
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
pub(crate) enum OpKind {
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
    Fn(Vec<OpKind>, Option<Vec<OpKind>>),
    /// #2823 — `option[T]` carries its payload kind. `Unknown` here would be
    /// fail-open in every operand guard, so `op + 1`, `takes_int(op)` and
    /// `var s string = op.unwrap()` all passed and failed `go build`.
    Option(Box<OpKind>),
    /// A declared curated foreign named type (`os.file`, `bufio.reader`,
    /// `net.conn`, `serde_json.value`, ...) — the names
    /// `foreign::foreign_named_type` classifies as opaque, own-handle or
    /// borrow-handle at the A5/move boundaries (#3521).
    ///
    /// Deliberately NOT `Unknown`: a named Go type is never assignable to or
    /// from the builtin scalar family, yet every kind check using these
    /// bindings fell through the `(Unknown, _)` fail-open arm, so
    /// `var f os.file` + `var n int = f` passed `hyb check` and died in
    /// `go build`.
    ///
    /// Cross-foreign pairs stay fail-open inside `kinds_conflict`: the kind
    /// model does not track interface satisfaction, and a concrete foreign
    /// value may legally target a foreign interface name (`io.reader`).
    Foreign(String),
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
            OpKind::Fn(res, params) => {
                let p_str = match params {
                    Some(ps) => ps.iter().map(|p| p.label()).collect::<Vec<_>>().join(", "),
                    None => "...".to_string(),
                };
                match res.len() {
                    0 => format!("fn({})", p_str),
                    1 => format!("fn({}) {}", p_str, res[0].label()),
                    _ => format!(
                        "fn({}) ({})",
                        p_str,
                        res.iter().map(|r| r.label()).collect::<Vec<_>>().join(", ")
                    ),
                }
            }
            OpKind::Option(e) => format!("option[{}]", e.label()),
            OpKind::Foreign(n) => n.clone(),
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
        // `Interface` on the `got` side is handled below (#2962): Hybrid has
        // no type assertion, so an `any` value never becomes a concrete type.
        //
        // Without this arm the U16 assignment check and the U9-U12 interface
        // classification contradict each other: `a` of type `any` resolves to
        // `Interface`, and `Interface` vs `Str` falls through to the mismatch
        // arm below, so every assignment to an `any` target would be rejected.
        // The accept-side corpus controls caught exactly that.
        (Interface, _) => false,
        // No value is involved; a count check owns that case.
        (Void, _) | (_, Void) => false,
        // #2962: the reverse direction is NOT assignable. A value of type
        // `any` (an `any` variable, param, result or `recover()`) needs a type
        // assertion to become a concrete type, and Hybrid has no assertion
        // syntax, so `var n int = a`, `take_int(a)`, `return a` from an `int`
        // fn and `[]int{a}` never build (`go build`: need type assertion).
        // Only `any`/`interface{}` resolve to `Interface` (named interfaces
        // and type parameters stay `Unknown`), so this claim is exact.
        // Equality (`a == 1`) stays accepted: equality_kinds_conflict needs
        // both directions to conflict and (Interface, _) above never does.
        (_, Interface) => true,
        // An untyped int constant is assignable to a float target
        // (`var f float64 = 1` builds). The kind model cannot tell an untyped
        // constant from an `int` variable, so this direction must stay allowed
        // or the check would reject buildable programs.
        // An untyped int constant is assignable to a float result
        // (`fn f() float64 { return 1 }` builds). The kind model cannot tell
        // an untyped constant from an `int` variable, so this direction must
        // stay allowed or the check would reject buildable programs.
        (Float, Int) => false,
        // A struct whose name did not resolve is not evidence of a mismatch.
        (Struct(None), _) | (_, Struct(None)) => false,
        // A declared curated foreign named type (`os.file`, `bufio.reader`,
        // ...) is a named Go type: it is never assignable to or from the
        // builtin scalar family, in either direction (`go build`: cannot use
        // f (variable of type os.File) as int value). Every OTHER pairing
        // with a foreign side stays fail-open — interface satisfaction is not
        // modelled here, so a closure may target `http.handler`, a reader may
        // target `io.reader`, and foreign values inside containers keep their
        // current permissive behavior; this arm claims exactly the scalar
        // interop that is provable.
        (Foreign(_), Int | Float | Bool | Str) | (Int | Float | Bool | Str, Foreign(_)) => true,
        (Foreign(_), _) | (_, Foreign(_)) => false,
        (a, b) if a == b => false,
        // Same container shape: recurse into the element kinds.
        (Slice(a), Slice(b)) | (Array(a), Array(b)) | (Chan(a), Chan(b)) => {
            kinds_conflict(a, b)
        }
        (Map(k1, v1), Map(k2, v2)) => kinds_conflict(k1, k2) || kinds_conflict(v1, v2),
        (Pointer(a), Pointer(b)) => kinds_conflict(a, b),
        // #2823: an option only satisfies an option (with a compatible
        // payload); every other pairing falls through to the mismatch arm, so
        // `op + 1` and `takes_int(op)` are refused exactly as Go refuses them.
        (Option(a), Option(b)) => kinds_conflict(a, b),
        (Fn(r1, p1), Fn(r2, p2)) => {
            if r1.len() != r2.len() {
                return true;
            }
            for (w, g) in r1.iter().zip(r2.iter()) {
                if w != &OpKind::Unknown && g != &OpKind::Unknown && w != g {
                    return true;
                }
            }
            if let (Some(ps1), Some(ps2)) = (p1, p2) {
                if ps1.len() != ps2.len() {
                    return true;
                }
                for (w, g) in ps1.iter().zip(ps2.iter()) {
                    if w != &OpKind::Unknown && g != &OpKind::Unknown && w != g {
                        return true;
                    }
                }
            }
            false
        }
        // Everything else is a shape or scalar-family mismatch Go refuses.
        _ => true,
    }
}

/// The composite container family of a rendered `TypeExpr` spelling, with the
/// parts split out (#3412). `None` = not a composite container (a bare name —
/// alias/type-parameter/named type — a `fn(...)`, an `option[...]`, …), where
/// the string model has no assignability claim and must fail open.
enum CompFamily<'a> {
    Chan {
        /// Direction is not compared here: direction mismatches are the
        /// E3012 coercion layer's single-diagnostic territory (#3412).
        _dir: &'a str,
        elem: &'a str,
    },
    Slice {
        elem: &'a str,
    },
    Array {
        len: Option<&'a str>,
        elem: &'a str,
    },
    Map {
        key: &'a str,
        value: &'a str,
    },
    Pointer {
        base: &'a str,
    },
}

/// Split a rendered composite spelling at its first balanced `]` from `from`.
fn split_balanced_bracket(s: &str, from: usize) -> Option<(usize, usize)> {
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut start = None;
    for (i, b) in bytes[from..].iter().enumerate() {
        let i = from + i;
        match b {
            b'[' => {
                if start.is_none() {
                    start = Some(i);
                }
                depth += 1;
            }
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some((start?, i));
                }
            }
            _ => {}
        }
    }
    None
}

impl<'a> CompFamily<'a> {
    /// Classify a rendered type spelling. Spellings come from
    /// `TypeExpr::render` (both sides of every comparison), so the forms
    /// below are exhaustive for composite containers.
    fn split(s: &'a str) -> Option<CompFamily<'a>> {
        if let Some(rest) = s.strip_prefix("<-chan ") {
            return Some(CompFamily::Chan {
                _dir: "recv",
                elem: rest,
            });
        }
        if let Some(rest) = s.strip_prefix("chan<- ") {
            return Some(CompFamily::Chan {
                _dir: "send",
                elem: rest,
            });
        }
        if let Some(rest) = s.strip_prefix("chan ") {
            return Some(CompFamily::Chan {
                _dir: "both",
                elem: rest,
            });
        }
        if let Some(rest) = s.strip_prefix("[]") {
            return Some(CompFamily::Slice { elem: rest });
        }
        if s.starts_with('[') {
            let (open, close) = split_balanced_bracket(s, 0)?;
            let len_txt = &s[open + 1..close];
            let len = if !len_txt.is_empty() && len_txt.bytes().all(|b| b.is_ascii_digit()) {
                Some(len_txt)
            } else {
                // An expression length (`[N]T`) has no textual truth here —
                // fail open on the length, the element still compares.
                None
            };
            return Some(CompFamily::Array {
                len,
                elem: &s[close + 1..],
            });
        }
        if let Some(_rest) = s.strip_prefix("map[") {
            let (open, close) = split_balanced_bracket(s, 3)?;
            return Some(CompFamily::Map {
                key: &s[open + 1..close],
                value: &s[close + 1..],
            });
        }
        if let Some(base) = s.strip_prefix('*') {
            return Some(CompFamily::Pointer { base });
        }
        None
    }
}

/// #1031 — the exact Go spelling of a predeclared numeric type, after the
/// emitter's SPEC→Go primitive mapping (`shapes::go_primitive_name`) and Go's
/// own identities (`byte` = `uint8`, `rune` = `int32`).
///
/// `OpKind` collapses every integer width into `Int` and both float widths
/// into `Float`, and `kinds_conflict` must allow `(Float, Int)` because an
/// untyped int constant is assignable to a float result. A TYPED numeric
/// value falls through exactly that gap: `n := 3; return n` from
/// `fn f() float64`, or an `int` param returned from `fn f() int64`, passes
/// the kind check and then fails `go build` with `cannot use n (variable of
/// type int) as float64 value in return statement`. This name lets the return
/// check compare exact types when — and only when — both sides are provably
/// typed.
///
/// `None` for anything that is not a predeclared numeric spelling, and for
/// `usize`/`isize`/`char`, which have no fixed Go spelling (fail-open).
fn canonical_numeric_name(name: &str) -> Option<&'static str> {
    Some(match name.trim() {
        "int" => "int",
        "int8" | "i8" => "int8",
        "int16" | "i16" => "int16",
        "int32" | "i32" | "rune" => "int32",
        "int64" | "i64" => "int64",
        "uint" => "uint",
        "uint8" | "u8" | "byte" => "uint8",
        "uint16" | "u16" => "uint16",
        "uint32" | "u32" => "uint32",
        "uint64" | "u64" => "uint64",
        "uintptr" => "uintptr",
        "float32" | "f32" => "float32",
        "float64" | "f64" => "float64",
        "complex64" | "c64" => "complex64",
        "complex128" | "c128" => "complex128",
        _ => return None,
    })
}

/// #2771 — `[3]int` yes; `[]int` (slice), `[N]int` (non-constant length), and
/// named types no — so only genuinely sized arrays are compared.
fn is_sized_array_spelling(s: &str) -> bool {
    let t = s.trim();
    if !t.starts_with('[') {
        return false;
    }
    let Some(close) = t.find(']') else {
        return false;
    };
    let head = &t[1..close];
    !head.is_empty() && head.chars().all(|c| c.is_ascii_digit())
}

/// #3280 — parse a fixed-size array type into `(dimension, element_type)`.
fn parse_sized_array(s: &str) -> Option<(&str, &str)> {
    let t = s.trim();
    if !t.starts_with('[') {
        return None;
    }
    let close = t.find(']')?;
    let head = &t[1..close];
    if head.is_empty() || !head.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((head, t[close + 1..].trim()))
}

/// #3280 — check whether two element type strings conflict (numeric scalar width,
/// nested slice elements, or fixed-size array dimensions and element types).
fn elem_types_conflict(want: &str, got: &str) -> bool {
    let w = want.trim();
    let g = got.trim();
    if let (Some(wn), Some(gn)) = (canonical_numeric_name(w), canonical_numeric_name(g)) {
        return wn != gn;
    }
    if let (Some(w_inner), Some(g_inner)) = (w.strip_prefix("[]"), g.strip_prefix("[]")) {
        return elem_types_conflict(w_inner, g_inner);
    }
    if let (Some((wl, we)), Some((gl, ge))) = (parse_sized_array(w), parse_sized_array(g)) {
        return wl != gl || elem_types_conflict(we, ge);
    }
    false
}

/// Is `e` provably a single-value expression (the #905/#903 binding-count
/// class)?
///
/// Conservative: `true` only for shapes that cannot yield two values in
/// any context. Calls, receives, and index expressions are excluded —
/// their counts are owned by the source-count rules (tuple.rs T23 and the
/// #926 void-position rule) — and an unresolved shape fails open.
fn is_definitely_single_value(e: &Expr) -> bool {
    match unwrap_parens(e) {
        Expr::Lit { .. }
        | Expr::Ident { .. }
        | Expr::Binary { .. }
        | Expr::Unary { .. }
        | Expr::AddrOf { .. }
        | Expr::Field { .. }
        | Expr::Interp { .. }
        | Expr::Slice { .. }
        | Expr::StructLit { .. }
        | Expr::SliceLit { .. }
        | Expr::MapLit { .. }
        | Expr::FnLit { .. } => true,
        // NOTE: `Expr::Try` is deliberately NOT here — `f()?` on a
        // (T1, T2, error) call yields TWO values after the unwrap (the
        // accept-corpus program 10-a5-http-handler-capture pins
        // `code, body := httptest.get(...)?` as legal), so a Try is a
        // source-count question, not a provably-single value.
        Expr::Move { operand, .. } => is_definitely_single_value(operand),
        _ => false,
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

/// Symmetrical conflict check for built-in `copy(dst, src)` element types.
///
/// In Go, `copy(dst, src)` requires both slices to have identical element types
/// (except for copying a string into a byte slice).
/// Unlike assignment or return, `copy` operates on existing slice values where untyped
/// constant adaptability does not apply: `copy([]float64, []int)` and `copy([]any, []int)`
/// are compile-time errors in Go.
fn copy_element_kinds_conflict(a: &OpKind, b: &OpKind) -> bool {
    use OpKind::*;
    match (a, b) {
        // Fail-open: unresolved kinds carry no claim.
        (Unknown, _) | (_, Unknown) => false,
        // Struct whose name did not resolve is not evidence of mismatch.
        (Struct(None), _) | (_, Struct(None)) => false,
        (a, b) if a == b => false,
        (Slice(a), Slice(b)) | (Array(a), Array(b)) | (Chan(a), Chan(b)) | (Pointer(a), Pointer(b)) => {
            copy_element_kinds_conflict(a, b)
        }
        (Map(k1, v1), Map(k2, v2)) => {
            copy_element_kinds_conflict(k1, k2) || copy_element_kinds_conflict(v1, v2)
        }
        (Fn(..), Fn(..)) => false,
        _ => true,
    }
}

/// Non-nilable types in Go and Hybrid cannot be compared to untyped nil (SPEC §3.1 N1).
/// Nilable types: pointer, interface, slice, map, channel, function.
/// `option[T]` lowers to the generic prelude struct `hyOption[T]`, which is non-nilable in Go.
fn is_non_nilable(k: &OpKind) -> bool {
    matches!(
        k,
        OpKind::Int
            | OpKind::Float
            | OpKind::Bool
            | OpKind::Str
            | OpKind::Struct(_)
            | OpKind::Array(_)
            | OpKind::Option(_)
    )
}

/// Does a type conversion `target(arg)` fail because the two types are incompatible in Go?
///
/// In Go, type conversions `T(x)` require compatible types:
/// - A struct cannot be converted from a non-struct (or unrelated struct).
/// - A bool cannot be converted to or from any other type.
/// - Numeric types (int, float) cannot convert to/from string, slice, map, chan, struct.
/// - Strings can only convert to/from integer (rune), byte/rune slices, or strings.
/// - Maps and channels cannot convert to/from scalars.
fn conversion_kinds_conflict(target: &OpKind, arg: &OpKind) -> bool {
    use OpKind::*;
    match (target, arg) {
        // Fail-open: unresolved or interface (any(x) is valid in Go)
        (Unknown, _) | (_, Unknown) => false,
        (Interface, _) => false,
        (a, b) if a == b => false,

        // #2482: An interface (`any`) value cannot be converted to a concrete
        // type in Go without a type assertion `x.(T)` (`cannot convert ... to type ...: need type assertion`).
        // Hybrid has no type assertion syntax, so converting `any` to any concrete
        // type is always invalid. The reverse direction (`any(concrete)`) is handled above.
        (other, Interface) if !matches!(other, Unknown | Interface) => true,

        // An `option[T]` value lowers to the generic prelude struct
        // `option[T]`; Go converts it to nothing but itself or an interface
        // (`u8(op)`, `float64(op)`, `string(op)` fail `go build` with
        // `cannot convert op (variable of struct type option[int])`).
        (Option(_), Option(_)) => false,
        (_, Option(_)) => true,
        (Option(_), other) if !matches!(other, Unknown | Option(_)) => true,

        // A declared curated foreign named type (os.file, net.conn, sync.mutex, ...)
        // lowers to a named Go type. Go forbids converting scalars or containers to/from
        // foreign structs, and only permits converting to `any` (handled above).
        (Foreign(_), other) if !matches!(other, Unknown | Foreign(_)) => true,
        (other, Foreign(_)) if !matches!(other, Unknown | Interface | Foreign(_)) => true,

        // Struct targets: Go forbids converting non-structs to structs.
        (Struct(_), other) if !matches!(other, Unknown | Struct(_)) => true,

        // Bool target: only bool can convert to bool.
        (Bool, other) if !matches!(other, Unknown | Bool) => true,
        // Bool argument: bool cannot convert to anything (except bool or interface).
        (other, Bool) if !matches!(other, Unknown | Bool | Interface) => true,

        // Numeric targets (Int, Float): can convert between Int and Float.
        // Cannot convert Str, Slice, Array, Map, Chan, Struct to numeric.
        (Int | Float, Str | Slice(_) | Array(_) | Map(_, _) | Chan(_) | Struct(_)) => true,

        // String target: can convert from Int (rune/byte code point) or Slice(Int) (byte/rune slice) or Str.
        // Cannot convert from Float, Bool, Map, Chan, Struct, Array, or non-int Slice.
        (Str, Float | Bool | Map(_, _) | Chan(_) | Struct(_) | Array(_)) => true,
        (Str, Slice(elem)) if !matches!(**elem, Unknown | Int) => true,

        // Slice target: can convert from Str (to []byte/[]rune), or Slice of identical element type.
        // Direct array-to-slice conversion `[]T(arr)` is invalid in Go (must write `arr[:]`).
        // Cannot convert from Int, Float, Bool, Map, Chan, Struct, Array.
        (Slice(_), Int | Float | Bool | Map(_, _) | Chan(_) | Struct(_) | Array(_)) => true,
        (Slice(elem), Str) if !matches!(**elem, Unknown | Int) => true,
        (Slice(a), Slice(b)) => a != b && **a != Unknown && **b != Unknown,

        // Array target: cannot convert from Str directly, or from non-container.
        // In Go 1.20+, slice-to-array conversion [N]T(slice) requires matching element types.
        (Array(_), Int | Float | Bool | Str | Map(_, _) | Chan(_) | Struct(_)) => true,
        (Array(a), Array(b)) | (Array(a), Slice(b)) => a != b && **a != Unknown && **b != Unknown,

        // Map target: cannot convert from Int, Float, Bool, Str, Slice, Array, Chan, Struct.
        // Converting between maps requires matching key and value types.
        (Map(_, _), Int | Float | Bool | Str | Slice(_) | Array(_) | Chan(_) | Struct(_)) => true,
        (Map(k1, v1), Map(k2, v2)) => {
            (k1 != k2 && **k1 != Unknown && **k2 != Unknown)
                || (v1 != v2 && **v1 != Unknown && **v2 != Unknown)
        }

        // Chan target: cannot convert from Int, Float, Bool, Str, Slice, Array, Map, Struct.
        // Converting between channels requires matching element types.
        (Chan(_), Int | Float | Bool | Str | Slice(_) | Array(_) | Map(_, _) | Struct(_)) => true,
        (Chan(a), Chan(b)) => a != b && **a != Unknown && **b != Unknown,

        // Pointer targets and arguments: Go only converts between pointers.
        // Direct pointer-to-pointer conversions require identical base types
        // (Go spec §Conversions).
        // A pointer cannot convert to or from any non-pointer type; the one
        // exception is an interface target (`any(p)`), handled above.
        (Pointer(a), Pointer(b)) => {
            if matches!(**a, Unknown | Struct(None)) || matches!(**b, Unknown | Struct(None)) {
                false
            } else {
                a != b
            }
        }
        (Pointer(_), _) | (_, Pointer(_)) => true,

        _ => false,
    }
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
        // #2823: an option's members stay fail-open — member-name checking on
        // `option[T]` is a separate concern (#1671), and this pass only needs
        // the payload kind for operand/argument/initializer checks.
        OpKind::Option(_) => false,
        // Foreign members live on the Go side: the kind model carries the
        // type NAME (for assignability) but no member table, so a member
        // lookup on `os.file` cannot claim absence (`f.read(...)` must keep
        // passing — unknown-FIELD/METHOD naming on foreign values is #2203's
        // territory, with its own surface tables).
        OpKind::Foreign(_) => false,
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

/// Mirror an assignment-target place back into expression form so the shared
/// index/slice checks can resolve declared lengths of the base (issue #999).
/// `None` means "cannot mirror" (address-of bases) — every caller stays
/// fail-open exactly as before the mirror existed.
fn place_as_expr(p: &Place) -> Option<Expr> {
    Some(match &p.kind {
        PlaceKind::Ident(name) => Expr::Ident {
            name: name.clone(),
            span: p.span,
        },
        PlaceKind::Index { base, index } => Expr::Index {
            base: Box::new(place_as_expr(base)?),
            index: Box::new(index.clone()),
            span: p.span,
        },
        PlaceKind::Field { base, name } => Expr::Field {
            base: Box::new(place_as_expr(base)?),
            name: name.clone(),
            span: p.span,
        },
        PlaceKind::AddrOf { .. } => return None,
    })
}

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
    is_untyped: bool,
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
    /// Parallel lexical scope stack tracking declared concrete type names (#2738).
    type_name_scopes: Vec<HashMap<String, String>>,
    /// `set[T]` bindings -> their element type text (`"int"`), scoped like
    /// `kind_scopes`. `set[T]` has no OpKind (it lowers to a prelude
    /// struct), so without this its element type was never checked (#2474).
    set_elem_scopes: Vec<HashMap<String, Option<String>>>,
    /// One layer per open scope, tracking `for ... in` element bindings
    /// whose kind is copy-semantics storage (`struct`/`array`). The emitter
    /// lowers them to Go's `for _, x := range xs`, so writes through the
    /// binding mutate the per-iteration copy and the collection never sees
    /// them — hy-check rejects those writes instead (#1480).
    copy_var_scopes: Vec<HashSet<String>>,
    /// File-level `const` kinds (#425).
    file_consts: HashMap<String, OpKind>,
    file_untyped_consts: HashSet<String>,
    /// Result kinds of the function whose body is currently being walked
    /// (#577 C9/C10). Empty for a fn declared with no results — which is also
    /// the state outside any body, so `Stmt::Return` can only ever see it
    /// while a body is open. `FnLit` bodies swap it like `fn_decl` does.
    fn_results: Vec<OpKind>,
    fn_result_type_names: Vec<String>,
    /// Type-parameter names of the function whose body is currently walked
    /// (`fn new[k comparable, v any]` → {k, v}). A composite literal inside a
    /// generic signature may spell those parameters (`map[k]v{}`, zero-value
    /// instantiations) — Go compiles them; bare `k` must NOT hit the
    /// value-position undeclared-type net. Swapped per body like `fn_results`.
    fn_type_params: HashSet<String>,
    /// Whether every result of the enclosing signature is named, so Go's
    /// naked `return` is legal (corpus 63).
    fn_results_all_named: bool,
    /// Whether the enclosing signature's last result is `error`, which the
    /// emitter zero-fills — a short return is then lowered to the full arity
    /// (`return v` in a `(value, error)` fn emits `return v, nil`, the E3
    /// sugar; corpus 13).
    fn_results_error_tail: bool,
    /// #1031 — parallel scope stack: the exact predeclared numeric type of a
    /// binding when it is provably TYPED (a typed param/result/declaration, or
    /// a `:=` whose initializer gives it Go's default type). Every binding
    /// site goes through `bind_kind`, which records `None` first, so an
    /// unclassified inner binding never inherits an outer binding's type.
    scalar_scopes: Vec<HashMap<String, Option<&'static str>>>,
    /// #2858 — parallel scope stack: the exact element/value numeric scalar of
    /// a container binding (slice, array, map) when it is provably TYPED.
    container_elem_scopes: Vec<HashMap<String, Option<&'static str>>>,
    /// #1031 — exact predeclared numeric type of each result of the enclosing
    /// signature (`None` = not a predeclared numeric spelling). Swapped
    /// together with `fn_results`.
    fn_result_scalars: Vec<Option<&'static str>>,
    /// Lexical nesting depth within loops (`for` / `loop`).
    loop_depth: usize,
    /// Lexical nesting depth within `select` statements.
    select_depth: usize,
    /// Lexical nesting depth within `managed` blocks (O6.1 hatch): while
    /// inside one, the sendability checks (#1112 `check_send_elem` and the
    /// #2817 `check_send_assignability`) are suppressed — the `managed` hatch
    /// deliberately relaxes sendability there and leaves the HYB-E8001 audit
    /// as the record (`fc_closure_boundary_send_inside_managed_escapes`).
    managed_depth: usize,
    defines: Vec<Define>,
    /// Every identifier mentioned in the file plus every dotted type root —
    /// the import-usage set (the emitter's declared-import header needs it).
    roots: HashSet<String>,
    diags: Vec<Diagnostic>,
    path: String,
    /// Composite element slots (items of a typed slice/array literal, keys
    /// and values of a map literal, and items of an elided slice literal in
    /// such a slot). hy-emit gives an untyped `[...]` there the EXPECTED
    /// element type, so only untyped literals OUTSIDE these slots take their
    /// type from their first item (`infer_slice_head`).
    elided_slots: HashSet<*const Expr>,
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
            type_name_scopes: Vec::new(),
            set_elem_scopes: Vec::new(),
            copy_var_scopes: Vec::new(),
            file_consts: HashMap::new(),
            file_untyped_consts: HashSet::new(),
            fn_results: Vec::new(),
            fn_result_type_names: Vec::new(),
            fn_type_params: HashSet::new(),
            fn_results_all_named: false,
            fn_results_error_tail: false,
            scalar_scopes: Vec::new(),
            container_elem_scopes: Vec::new(),
            fn_result_scalars: Vec::new(),
            loop_depth: 0,
            select_depth: 0,
            managed_depth: 0,
            defines: Vec::new(),
            roots: HashSet::new(),
            diags: Vec::new(),
            path: ck.path.clone(),
            elided_slots: HashSet::new(),
        };
        for d in &file.decls {
            if let Decl::Const(cd) = d {
                let k = cd
                    .ty
                    .as_ref()
                    .map(|t| c.kind_of_ty(t, 0))
                    .unwrap_or_else(|| c.kind_of_expr(&cd.value));
                c.file_consts.insert(cd.name.clone(), k);
                if cd.ty.is_none() {
                    c.file_untyped_consts.insert(cd.name.clone());
                }
            }
        }
        for d in &file.decls {
            match d {
                Decl::Import { .. } => {}
                Decl::Const(cd) => {
                    c.ty_opt(&cd.ty);
                    c.expr(&cd.value);
                    c.check_const_decl(cd);
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
        self.type_name_scopes.push(HashMap::new());
        self.scalar_scopes.push(HashMap::new());
        self.container_elem_scopes.push(HashMap::new());
        self.set_elem_scopes.push(HashMap::new());
        self.copy_var_scopes.push(HashSet::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
        self.kind_scopes.pop();
        self.type_name_scopes.pop();
        self.scalar_scopes.pop();
        self.container_elem_scopes.pop();
        self.set_elem_scopes.pop();
        self.copy_var_scopes.pop();
    }

    fn bind_type_name(&mut self, name: &str, type_name: String) {
        if name != "_" {
            if let Some(top) = self.type_name_scopes.last_mut() {
                top.insert(name.to_string(), type_name);
            }
        }
    }

    fn lookup_type_name(&self, name: &str) -> Option<String> {
        for layer in self.type_name_scopes.iter().rev() {
            if let Some(t) = layer.get(name) {
                return Some(t.clone());
            }
        }
        None
    }

    fn slice_element_type_name(&self, e: &Expr) -> Option<String> {
        let bare = unwrap_parens(e);
        match bare {
            Expr::Ident { name, .. } => {
                if let Some(tname) = self.lookup_type_name(name) {
                    if let Some(elem) = tname.strip_prefix("[]") {
                        return Some(elem.to_string());
                    }
                    if let Some(elem) = tname.strip_prefix("[") {
                        if let Some(close) = elem.find(']') {
                            return Some(elem[close + 1..].to_string());
                        }
                    }
                }
                None
            }
            Expr::SliceLit { elem: Some(t), .. } => {
                match t.as_ref() {
                    TypeExpr::Array { elem, .. } => Some(elem.render()),
                    TypeExpr::Slice { elem, .. } => Some(elem.render()),
                    _ => {
                        let r = t.render();
                        if let Some(elem) = r.strip_prefix("[]") {
                            Some(elem.to_string())
                        } else if let Some(elem) = r.strip_prefix("[") {
                            if let Some(close) = elem.find(']') {
                                Some(elem[close + 1..].to_string())
                            } else {
                                Some(r)
                            }
                        } else {
                            Some(r)
                        }
                    }
                }
            }
            Expr::Slice { base, .. } => self.slice_element_type_name(base),
            Expr::Try { operand, .. } => self.slice_element_type_name(operand),
            Expr::Call { callee, args, .. } => {
                let unwrapped_callee = unwrap_parens(callee);
                if let Expr::Ident { name, .. } = unwrapped_callee {
                    if name == "make" {
                        if let Some(first) = args.first() {
                            return self.slice_element_type_name(&first.value);
                        }
                    }
                } else if let Expr::Field { base, name, .. } = unwrapped_callee {
                    if let Expr::Ident { name: pkg, .. } = unwrap_parens(base) {
                        if pkg == "io" && name == "read_all" {
                            return Some("byte".to_string());
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// #2817 — the exact numeric element type of a channel expression, when
    /// the channel is a plain identifier whose declared element type is a
    /// predeclared numeric type. `var ch chan int64` renders its declared type
    /// into `type_name_scopes` as `chan int64`; direction qualifiers
    /// (`<-chan int`, `chan<- int`) normalize to the element. Named types,
    /// non-numeric elements, and unresolved bindings return `None` so the
    /// caller stays fail-open.
    fn chan_element_scalar(&self, e: &Expr) -> Option<&'static str> {
        let Expr::Ident { name, .. } = unwrap_parens(e) else {
            return None;
        };
        let tname = self.lookup_type_name(name)?;
        let t = tname.trim();
        let t = t.strip_prefix("<-").unwrap_or(t).trim();
        let t = t.strip_prefix("chan")?;
        let t = t.trim_start();
        let t = t.strip_prefix("<-").unwrap_or(t).trim();
        canonical_numeric_name(t)
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
            // #1031: every binding shadows an outer exact numeric type until
            // `set_scalar` proves otherwise.
            if let Some(top) = self.scalar_scopes.last_mut() {
                top.insert(name.to_string(), None);
            }
            // #2858: every binding shadows an outer container element scalar.
            if let Some(top) = self.container_elem_scopes.last_mut() {
                top.insert(name.to_string(), None);
            }
            // Every (re)binding shadows an outer set binding; callers that
            // know the new binding is a set record it via bind_set_elem.
            if let Some(top) = self.set_elem_scopes.last_mut() {
                top.insert(name.to_string(), None);
            }
        }
    }

    fn bind_set_elem(&mut self, name: &str, elem: Option<String>) {
        if name != "_" && elem.is_some() {
            if let Some(top) = self.set_elem_scopes.last_mut() {
                top.insert(name.to_string(), elem);
            }
        }
    }

    /// `set[int]` -> `"int"`.
    fn set_elem_of_ty(ty: &TypeExpr) -> Option<String> {
        if let TypeExpr::Named { name, .. } = ty {
            return name
                .strip_prefix("set[")
                .and_then(|r| r.strip_suffix(']'))
                .map(|t| t.to_string());
        }
        None
    }

    /// The element type text of a set-valued expression, when provable.
    fn set_elem_of_expr(&self, e: &Expr) -> Option<String> {
        match unwrap_parens(e) {
            Expr::Ident { name, .. } => {
                for layer in self.set_elem_scopes.iter().rev() {
                    if let Some(v) = layer.get(name) {
                        return v.clone();
                    }
                }
                None
            }
            Expr::StructLit { ty: Some(t), .. } => Self::set_elem_of_ty(t),
            // `mk()` where `fn mk() set[T]` — the declared single result.
            Expr::Call { callee, .. } => match unwrap_parens(callee) {
                Expr::Ident { name, .. } if self.lookup_kind(name).is_none() => self
                    .ck
                    .fns
                    .get(name.as_str())
                    .filter(|f| f.results.len() == 1)
                    .and_then(|f| Self::set_elem_of_ty(&f.results[0].ty)),
                _ => None,
            },
            _ => None,
        }
    }

    /// #2474: `s.insert(v)` / `s.contains(v)` / `s.remove(v)` on a `set[T]`
    /// need `v` of type `T`; the prelude's Go signature takes `T`, so a
    /// mismatched argument passed `hyb check` and failed `go build`.
    fn check_set_method_arg(&mut self, callee: &Expr, args: &[CallArg]) {
        let Expr::Field { base, name, .. } = unwrap_parens(callee) else {
            return;
        };
        if !matches!(name.as_str(), "insert" | "contains" | "remove") || args.len() != 1 {
            return;
        }
        let Some(elem) = self.set_elem_of_expr(base) else {
            return;
        };
        let want = self.kind_of_named(&elem, 0);
        let got = self.kind_of_expr(&args[0].value);
        let int_to_float_conflict = want == OpKind::Float
            && got == OpKind::Int
            && !self.is_untyped_int_const(&args[0].value);
        if kinds_conflict(&want, &got) || int_to_float_conflict {
            self.emit_e1002(
                args[0].value.span(),
                format!(
                    "cannot use `{}` (type `{}`) as `{}` value in argument to `{}.{}` on `set[{}]` (HYB-E1002)",
                    args[0].value.render(),
                    got.label(),
                    want.label(),
                    base.render(),
                    name,
                    elem
                ),
                "a set's insert/contains/remove take a value of the set's element type",
            );
        }
    }

    /// #2915: `opt.or(default)` on an `option[T]` needs `default` of type `T`.
    fn check_option_method_arg(&mut self, callee: &Expr, args: &[CallArg]) {
        let Expr::Field { base, name, .. } = unwrap_parens(callee) else {
            return;
        };
        if name != "or" || args.len() != 1 {
            return;
        }
        let bk = self.kind_of_expr(base);
        let OpKind::Option(elem) = bk else {
            return;
        };
        let want = *elem;
        let got = self.kind_of_expr(&args[0].value);
        let int_to_float_conflict = want == OpKind::Float
            && got == OpKind::Int
            && !self.is_untyped_int_const(&args[0].value);
        if kinds_conflict(&want, &got) || int_to_float_conflict {
            self.emit_e1002(
                args[0].value.span(),
                format!(
                    "cannot use `{}` (type `{}`) as `{}` value in argument to `{}.or` on `option[{}]` (HYB-E1002)",
                    args[0].value.render(),
                    got.label(),
                    want.label(),
                    base.render(),
                    want.label()
                ),
                "argument to `option.or` must match the option payload type",
            );
        }
    }

    /// #1031 — record the exact numeric type of a binding that `bind_kind`
    /// just made in the current scope.
    fn set_scalar(&mut self, name: &str, scalar: Option<&'static str>) {
        if name != "_" {
            if let Some(top) = self.scalar_scopes.last_mut() {
                top.insert(name.to_string(), scalar);
            }
        }
    }

    /// #1031 — the exact numeric type of a binding, if provably typed. A name
    /// that is not a binding (e.g. a file-level const, which may be untyped)
    /// stays `None`.
    fn lookup_scalar(&self, name: &str) -> Option<&'static str> {
        for layer in self.scalar_scopes.iter().rev() {
            if let Some(s) = layer.get(name) {
                return *s;
            }
        }
        None
    }

    /// #2858 — record the exact element/value numeric type of a container
    /// binding (slice, array, map) made in the current scope.
    fn set_container_elem_scalar(&mut self, name: &str, scalar: Option<&'static str>) {
        if name != "_" {
            if let Some(top) = self.container_elem_scopes.last_mut() {
                top.insert(name.to_string(), scalar);
            }
        }
    }

    /// #2858 — the exact element/value numeric scalar of a container binding.
    fn lookup_container_elem_scalar(&self, name: &str) -> Option<&'static str> {
        for layer in self.container_elem_scopes.iter().rev() {
            if let Some(s) = layer.get(name) {
                return *s;
            }
        }
        None
    }

    /// #2770 — the canonical numeric spelling of a simple local target, for the
    /// assignment/argument scalar comparison. Non-ident places fail open.
    fn place_scalar(&self, p: &Place) -> Option<&'static str> {
        match &p.kind {
            PlaceKind::Ident(n) => self.lookup_scalar(n),
            PlaceKind::Field { base, name } => {
                let bk = self.kind_of_place(base);
                self.struct_field_scalar(&bk, name)
            }
            PlaceKind::Index { base, .. } => {
                let bk = self.kind_of_place(base);
                if bk == OpKind::Str {
                    Some("uint8")
                } else if let Some(be) = place_as_expr(base) {
                    self.container_elem_scalar_of_expr(&be)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// #2771 — the declared Go spelling of a value whose type carries detail
    /// that `OpKind` drops, notably an array's length (`[3]int` vs `[4]int`).
    /// Identifiers bound with an explicit type name or explicit array literals
    /// are known; everything else (conversions, calls) fails open.
    fn typed_value_type_name(&self, e: &Expr) -> Option<String> {
        let bare = unwrap_parens(e);
        if let Some((len_opt, inner_elem, array_ty)) = bare.as_array_lit() {
            // #3599: For `[...]int{1, 2, 3}`, len_opt is None; compute length from items.len().
            // For `[N]int{1, 2, 3}` where N is a constant, fold N to its integer length.
            if let Some(l) = len_opt {
                if let Some(v) = crate::const_overflow::const_int_value(l)
                    .or_else(|| self.ck.const_int_value_at(l, l.span().start, 0))
                {
                    if v >= 0 {
                        return Some(format!("[{}]{}", v, inner_elem.render()));
                    }
                }
            } else if let Expr::SliceLit { items, .. } = bare {
                return Some(format!("[{}]{}", items.len(), inner_elem.render()));
            }
            return Some(array_ty.render());
        }
        match bare {
            Expr::Ident { name, .. } => self.lookup_type_name(name),
            Expr::Field { base, name, .. } => {
                let bk = self.kind_of_expr(base);
                self.struct_field_type_name(&bk, name)
            }
            Expr::SliceLit { elem: Some(t), .. } => Some(format!("[]{}", t.render())),
            _ => None,
        }
    }

    /// Returns the type parameter name if `e` statically evaluates to an in-scope generic type parameter.
    fn expr_type_param(&self, e: &Expr) -> Option<String> {
        let bare = unwrap_parens(e);
        match bare {
            Expr::Ident { name, .. } => {
                if self.fn_type_params.contains(name) {
                    return Some(name.clone());
                }
                if let Some(tn) = self.lookup_type_name(name) {
                    if self.fn_type_params.contains(&tn) {
                        return Some(tn);
                    }
                }
                None
            }
            Expr::Field { base, name, .. } => {
                let bk = self.kind_of_expr(base);
                if let Some(tn) = self.struct_field_type_name(&bk, name) {
                    if self.fn_type_params.contains(&tn) {
                        return Some(tn);
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// #2771 — the declared Go spelling of a simple local assignment target.
    fn place_type_name(&self, p: &Place) -> Option<String> {
        match &p.kind {
            PlaceKind::Ident(n) => self.lookup_type_name(n),
            PlaceKind::Field { base, name } => {
                let bk = self.kind_of_place(base);
                self.struct_field_type_name(&bk, name)
            }
            PlaceKind::Index { base, .. } => {
                if let Some(base_tname) = self.place_type_name(base) {
                    let s = base_tname.trim();
                    if let Some(elem) = s.strip_prefix("[]") {
                        return Some(elem.trim().to_string());
                    }
                    if s.starts_with('[') {
                        if let Some(close) = s.find(']') {
                            let elem = s[close + 1..].trim();
                            if !elem.is_empty() {
                                return Some(elem.to_string());
                            }
                        }
                    }
                    if let Some(rest) = s.strip_prefix("map[") {
                        let mut depth = 1;
                        for (idx, ch) in rest.char_indices() {
                            if ch == '[' {
                                depth += 1;
                            } else if ch == ']' {
                                depth -= 1;
                                if depth == 0 {
                                    let val = rest[idx + 1..].trim();
                                    if !val.is_empty() {
                                        return Some(val.to_string());
                                    }
                                    break;
                                }
                            }
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// #2771 — two KNOWN and DIFFERENT sized-array spellings. Slices, named
    /// types and unparsed lengths fail open so aliases never false-reject.
    fn sized_array_names_differ(&self, a: Option<&str>, b: Option<&str>) -> bool {
        matches!((a, b), (Some(x), Some(y))
            if x != y && is_sized_array_spelling(x) && is_sized_array_spelling(y))
    }

    /// #3412 — deep composite-element conflict: the kind model collapses
    /// container inners (`OpKind::Chan(OpKind::Int)` fits `chan int64` as
    /// well as `chan int32`), so a typed composite element whose INNER type
    /// differs passed the `ik == expected` arm and failed `go build`
    /// (`cannot use bad (variable of type chan int64) as chan int32 value in
    /// slice literal`). Compare the resolved spellings part by part.
    ///
    /// Fail-open everywhere the string model cannot see Go assignability: a
    /// bare-word (alias / named / type-parameter) spelling on either side, a
    /// family mismatch (the kind layer already owns those), any type
    /// parameter appearing in either spelling, and non-numeric array
    /// lengths. Only a provable textual part mismatch conflicts.
    fn deep_composite_conflict(&self, want: &TypeExpr, got: Option<&str>) -> Option<&'static str> {
        let got = got?;
        if got.is_empty() {
            return None;
        }
        let want_resolved = self.ck.resolve_type_alias(want);
        let want_s = want_resolved.render();
        if want_s == got {
            return None;
        }
        if self
            .fn_type_params
            .iter()
            .any(|p| want_s.contains(p.as_str()) || got.contains(p.as_str()))
        {
            return None;
        }
        match (CompFamily::split(&want_s), CompFamily::split(got)) {
            (
                Some(CompFamily::Chan {
                    elem: we,
                    ..
                }),
                Some(CompFamily::Chan {
                    elem: ge,
                    ..
                }),
            ) => {
                // Direction mismatches here are the E3012 coercion layer's
                // territory (its slice/map-literal arms already reject
                // them); a second E1002 on the same span is a double
                // report. Only an ELEMENT type difference is ours.
                (we != ge).then_some("channel element types differ")
            }
            (Some(CompFamily::Slice { elem: we }), Some(CompFamily::Slice { elem: ge })) => {
                (we != ge).then_some("slice element types differ")
            }
            (
                Some(CompFamily::Array {
                    len: wl,
                    elem: we,
                }),
                Some(CompFamily::Array {
                    len: gl,
                    elem: ge,
                }),
            ) => {
                if let (Some(a), Some(b)) = (wl, gl) {
                    if a != b {
                        return Some("array lengths differ");
                    }
                }
                (we != ge).then_some("array element types differ")
            }
            (
                Some(CompFamily::Map {
                    key: wk,
                    value: wv,
                }),
                Some(CompFamily::Map {
                    key: gk,
                    value: gv,
                }),
            ) => {
                if wk != gk {
                    Some("map key types differ")
                } else if wv != gv {
                    Some("map value types differ")
                } else {
                    None
                }
            }
            (Some(CompFamily::Pointer { base: wb }), Some(CompFamily::Pointer { base: gb })) => {
                (wb != gb).then_some("pointer base types differ")
            }
            // Family mismatch or a non-composite side: the kind layer owns
            // the former and the latter has no string-level claim.
            _ => None,
        }
    }
    /// #1031 — the exact predeclared numeric spelling of a type expression.
    /// A user-declared struct/enum/alias of that name is not the predeclared
    /// type, so it fails open.
    fn scalar_of_type_name(&self, name: &str) -> Option<&'static str> {
        if self.ck.structs.contains_key(name)
            || self.ck.enums.contains_key(name)
            || self.ck.type_aliases.contains_key(name)
        {
            return None;
        }
        canonical_numeric_name(name)
    }

    fn scalar_of_ty(&self, ty: &TypeExpr) -> Option<&'static str> {
        let TypeExpr::Named { name, .. } = ty else {
            return None;
        };
        self.scalar_of_type_name(name)
    }

    /// #2858 — the canonical numeric element/value type of a container type expression.
    fn container_elem_scalar_of_ty(&self, ty: &TypeExpr) -> Option<&'static str> {
        let resolved = self.ck.resolve_type_alias(ty);
        match &resolved {
            TypeExpr::Slice { elem, .. } | TypeExpr::Array { elem, .. } => {
                self.scalar_of_ty(elem)
            }
            TypeExpr::Map { value, .. } => self.scalar_of_ty(value),
            TypeExpr::Pointer { to, .. } => match to.as_ref() {
                TypeExpr::Array { elem, .. } => self.scalar_of_ty(elem),
                _ => None,
            },
            _ => None,
        }
    }

    /// #2858 — resolve the exact element/value numeric scalar of an expression representing a container.
    fn container_elem_scalar_of_expr(&self, e: &Expr) -> Option<&'static str> {
        match unwrap_parens(e) {
            Expr::Ident { name, .. } => self.lookup_container_elem_scalar(name),
            Expr::SliceLit { elem, .. } => {
                if let Some((_, inner_elem, _)) = e.as_array_lit() {
                    self.scalar_of_ty(inner_elem)
                } else if let Some(el) = elem {
                    self.scalar_of_ty(el)
                } else {
                    None
                }
            }
            Expr::MapLit { value, .. } => self.scalar_of_ty(value),
            Expr::Slice { base, .. } => self.container_elem_scalar_of_expr(base),
            Expr::Field { base, name, .. } => {
                let bk = self.kind_of_expr(base);
                self.struct_field_container_elem_scalar(&bk, name)
            }
            Expr::Call { callee, args, .. } => {
                let bare_callee = unwrap_parens(callee);
                if let Expr::Ident { name, .. } = bare_callee {
                    if name == "make" {
                        if let Some(first) = args.first() {
                            return self.container_elem_scalar_of_expr(&first.value);
                        }
                    }
                    if let Some(fd) = self.ck.fns.get(name.as_str()) {
                        if fd.receiver.is_none() && fd.type_params.is_empty() && fd.results.len() == 1 {
                            return self.container_elem_scalar_of_ty(&fd.results[0].ty);
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// #1031 — exact result types of a call whose callee is provably known:
    /// a non-generic top-level function, or a predeclared numeric conversion
    /// `T(x)`. A local binding of the callee name shadows both (fail-open).
    fn call_result_scalars(
        &self,
        callee: &Expr,
        args: &[CallArg],
    ) -> Option<Vec<Option<&'static str>>> {
        let Expr::Ident { name, .. } = unwrap_parens(callee) else {
            return None;
        };
        if self
            .scopes
            .iter()
            .any(|layer| layer.contains_key(name.as_str()))
        {
            return None;
        }
        if let Some(fd) = self.ck.fns.get(name.as_str()) {
            if fd.receiver.is_some() || !fd.type_params.is_empty() {
                return None;
            }
            return Some(fd.results.iter().map(|r| self.scalar_of_ty(&r.ty)).collect());
        }
        if name == "min" || name == "max" {
            let res_scalar = args
                .iter()
                .find_map(|a| self.typed_value_scalar(&a.value));
            return Some(vec![res_scalar]);
        }
        if args.len() == 1
            && !self.ck.structs.contains_key(name.as_str())
            && !self.ck.enums.contains_key(name.as_str())
            && !self.ck.type_aliases.contains_key(name.as_str())
        {
            if let Some(s) = canonical_numeric_name(name) {
                return Some(vec![Some(s)]);
            }
        }
        None
    }

    /// #1031 — the exact numeric type of a value that is provably TYPED.
    /// Literals are untyped constants (assignable to any numeric type), so
    /// they are deliberately `None` here.
    fn typed_value_scalar(&self, e: &Expr) -> Option<&'static str> {
        match unwrap_parens(e) {
            Expr::Ident { name, .. } => self.lookup_scalar(name),
            // Typed arithmetic keeps its operand type: in any program that
            // builds, both operands share one type or one is an untyped
            // constant, so the typed side names the result type. A shift's
            // result takes the type of its left operand alone.
            Expr::Binary { op, lhs, rhs, .. } => match op {
                BinOp::Add
                | BinOp::Sub
                | BinOp::Mul
                | BinOp::Div
                | BinOp::Mod
                | BinOp::BitOr
                | BinOp::BitXor
                | BinOp::BitAnd => self
                    .typed_value_scalar(lhs)
                    .or_else(|| self.typed_value_scalar(rhs)),
                BinOp::Shl | BinOp::Shr => self.typed_value_scalar(lhs),
                _ => None,
            },
            Expr::Unary {
                op: UnOp::Neg,
                operand,
                ..
            } => self.typed_value_scalar(operand),
            Expr::Call { callee, args, .. } => match self.call_result_scalars(callee, args) {
                Some(rs) if rs.len() == 1 => rs[0],
                _ => None,
            },
            // #2858: struct field scalar resolution
            Expr::Field { base, name, .. } => {
                let bk = self.kind_of_expr(base);
                self.struct_field_scalar(&bk, name)
            }
            // #2858: index into slice/array/map/string
            Expr::Index { base, .. } => {
                if self.kind_of_expr(base) == OpKind::Str {
                    Some("uint8")
                } else {
                    self.container_elem_scalar_of_expr(base)
                }
            }
            _ => None,
        }
    }

    /// #2782 / #2788 — two TYPED values whose canonical numeric types differ
    /// (`int` vs `int64`, `float64` vs `float32`). The scalar helpers return
    /// `None` for an untyped constant, so an untyped initializer stays
    /// assignable (`x: 1`, `[]int64{42}`, `map[string]float32{"a": 1.5}`), and
    /// `byte`/`uint8` (or `rune`/`int32`) share one canonical name so the
    /// genuine aliases never conflict. One shared boundary check for the
    /// struct-literal (#2782), append (#2796) and slice/map-literal (#2788)
    /// arms.
    fn scalar_names_conflict(
        &self,
        want: Option<&'static str>,
        got: Option<&'static str>,
    ) -> bool {
        matches!((want, got), (Some(w), Some(g)) if w != g)
    }

    /// #1031 — the type a `x := v` / `var x = v` binding takes from its
    /// initializer: Go's default type for an untyped numeric literal (`int`,
    /// `float64`), or the initializer's own type when it is typed.
    fn initializer_scalar(&self, e: &Expr) -> Option<&'static str> {
        match unwrap_parens(e) {
            Expr::Lit {
                kind: LitKind::Int(_),
                ..
            } => Some("int"),
            Expr::Lit {
                kind: LitKind::Float(_),
                ..
            } => Some("float64"),
            // Go's default type for an untyped rune constant is `rune`
            // (`int32`): `r := 'x'` binds an int32, so `r + i` (i int) or
            // `var n int = r` must not pass as a same-width pair.
            Expr::Lit {
                kind: LitKind::Char(_),
                ..
            } => Some("int32"),
            Expr::Unary {
                op: UnOp::Neg,
                operand,
                ..
            } => match unwrap_parens(operand) {
                Expr::Lit {
                    kind: LitKind::Int(_),
                    ..
                } => Some("int"),
                Expr::Lit {
                    kind: LitKind::Float(_),
                    ..
                } => Some("float64"),
                Expr::Lit {
                    kind: LitKind::Char(_),
                    ..
                } => Some("int32"),
                _ => self.typed_value_scalar(operand),
            },
            other => self.typed_value_scalar(other),
        }
    }

    /// #2770: two TYPED values of different canonical numeric types (e.g.
    /// `float32` vs `float64`, `int` vs `int64`) are distinct Go types and
    /// cannot combine, compare, or assign directly. `typed_value_scalar` is
    /// `None` for an untyped constant, so `f + 1.5` / `i + 1` stay legal, and
    /// `byte`/`uint8` (or `rune`/`int32`) share one canonical name so the
    /// genuine aliases never conflict.
    fn scalar_conflict(&self, a: &Expr, b: &Expr) -> bool {
        match (self.typed_value_scalar(a), self.typed_value_scalar(b)) {
            (Some(x), Some(y)) => x != y,
            _ => false,
        }
    }

    /// #1031 — exact numeric types for the names of a local declaration.
    fn infer_local_scalars(&self, l: &LocalDecl) -> Vec<Option<&'static str>> {
        let n = l.names.len();
        if let Some(ty) = &l.ty {
            return vec![self.scalar_of_ty(ty); n];
        }
        // An untyped `const c = 3` stays an untyped constant: assignable to
        // any numeric result, so it must not be pinned to `int`.
        if l.kind == LocalKind::Const {
            return vec![None; n];
        }
        if n == l.values.len() {
            return l.values.iter().map(|v| self.initializer_scalar(v)).collect();
        }
        if n >= 2 && l.values.len() == 1 {
            if let Expr::Call { callee, args, .. } = unwrap_parens(&l.values[0]) {
                if let Some(rs) = self.call_result_scalars(callee, args) {
                    if rs.len() == n {
                        return rs;
                    }
                }
            }
            // #2858: comma-ok map index (val, ok := m["k"])
            if n == 2 {
                if let Expr::Index { base, .. } = unwrap_parens(&l.values[0]) {
                    let elem_sc = if self.kind_of_expr(base) == OpKind::Str {
                        Some("uint8")
                    } else {
                        self.container_elem_scalar_of_expr(base)
                    };
                    return vec![elem_sc, None];
                }
            }
        }
        vec![None; n]
    }

    fn lookup_kind(&self, name: &str) -> Option<OpKind> {
        for layer in self.kind_scopes.iter().rev() {
            if let Some(k) = layer.get(name) {
                return Some(k.clone());
            }
        }
        self.file_consts.get(name).cloned()
    }

    /// For `fmt` printf-family format operands: the kind label when `e` is
    /// statically known to have a non-`string` kind, `None` otherwise.
    /// `Str` (legit), `Unknown` (unresolvable), and `Interface` (`any`,
    /// which Go accepts as a printf operand) stay permissive — mirroring the
    /// conservative accept-side shape used everywhere in this file.
    fn nonstring_format_kind(&self, e: &Expr) -> Option<String> {
        match self.kind_of_expr(e) {
            OpKind::Str | OpKind::Unknown | OpKind::Interface => None,
            k => Some(k.label()),
        }
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
            is_untyped: false,
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

    /// #778 — a TYPE NAME used where a VALUE is required.
    ///
    /// `struct holder { n int }` followed by `x := holder` passes `hyb check`
    /// and the emitter prints the identifier verbatim, so `go build` fails with
    /// `holder (type) is not an expression`. Go has no value named `holder`,
    /// so this is not an `Unknown` fail-open case: the shape IS known and the
    /// value does not exist.
    ///
    /// Deliberately narrow, because a type name is a legal operand in a
    /// CONVERSION (`holder(x)`, `int(3.7)`). The call arm handles its callee
    /// separately, so this is only reached for a genuine value position.
    ///
    /// Fail-open on anything that could be a value: a local/parameter/closure
    /// binding shadows the type name (`holder := 5` is a legal variable), and
    /// an unknown name is another rule's business (unused-binding, O4.9).
    fn check_type_name_in_value_position(&mut self, name: &str, span: Span) {
        // A binding in any enclosing scope shadows the type name.
        if self
            .scopes
            .iter()
            .rev()
            .any(|layer| layer.contains_key(name))
        {
            return;
        }
        // A file-level const of the same name is a value, not the type.
        if self.ck.consts.contains_key(name) {
            return;
        }
        // A top-level fn of the same name is callable, not the type.
        let head = Checker::base_named_type_str(name);
        if self.ck.fns.contains_key(name) || self.ck.fns.contains_key(head) {
            return;
        }
        // The name must actually resolve to a declared type — a struct, an
        // enum, a type alias, or a built-in conversion target. Anything else
        // stays fail-open.
        let is_declared_type = self.ck.structs.contains_key(name)
            || self.ck.struct_type_params.contains_key(head)
            || self.ck.enums.contains_key(name)
            || self.ck.type_aliases.contains_key(name)
            || name == "any"
            || name == "error"
            || crate::const_init::is_conversion_type_name(name);
        if !is_declared_type {
            return;
        }
        self.emit_e1002(
            span,
            format!(
                "`{}` is a type, not a value — Go refuses `{} (type) is not an expression` (HYB-E1002)",
                name, name
            ),
            "a type name can only be used in a type position or as a conversion callee (`T(x)`)",
        );
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

    fn check_signature_bindings(
        &mut self,
        receiver: Option<&str>,
        params: &[Param],
        results: &[FnResult],
        span: Span,
    ) {
        let mut seen = HashSet::new();
        let receiver = receiver.map(|name| (name, span)).into_iter();
        let params = params.iter().map(|p| (p.name.as_str(), p.span));
        let results = results
            .iter()
            .filter_map(|r| r.name.as_deref().map(|name| (name, r.ty.span())));
        for (name, name_span) in receiver.chain(params).chain(results) {
            if name != "_" && !seen.insert(name) {
                self.diags.push(
                    Diagnostic::error(
                        "HYB-E1002",
                        self.path.clone(),
                        name_span,
                        format!(
                            "function signature declares `{name}` more than once — Go refuses `{name} redeclared in this block` (HYB-E1002)"
                        ),
                    )
                    .with_notes(vec![
                        "receiver, parameter, and named-result identifiers share one function scope; rename the repeated declaration or use `_` for an unused parameter"
                            .to_string(),
                    ]),
                );
            }
        }
    }

    // -- declarations --------------------------------------------------------

    fn fn_decl(&mut self, f: &FnDecl) {
        if f.receiver.is_none() {
            if f.name == "main" {
                if !f.params.is_empty() || !f.type_params.is_empty() {
                    self.diags.push(
                    Diagnostic::error(
                        "HYB-E1002",
                        self.path.clone(),
                        f.span,
                        "`fn main` must have no parameters — Go requires `func main` to have no arguments and no return values (HYB-E1002)".to_string(),
                    )
                    .with_notes(vec![
                        "remove parameters from `fn main()`, or read CLI arguments via `os.args()`".to_string(),
                    ]),
                );
                } else {
                    let body_has_try = f.body.as_ref().map_or(false, block_has_try);
                    let main_needs_wrapper = !f.results.is_empty() || body_has_try;
                    if main_needs_wrapper {
                        if self.is_os_shadowed() {
                            self.emit_e1002(
                                f.span,
                                "cannot declare `fn main` with error/exit return or `?` because `os` is shadowed by an in-scope declaration (HYB-E1002)".to_string(),
                                "error-returning `fn main` lowers to `os.Exit` and `os.Stderr`; rename the shadowing `os` declaration so the standard `os` package is accessible",
                            );
                        }
                        if self.is_filepath_shadowed() {
                            self.emit_e1002(
                                f.span,
                                "cannot declare `fn main` with error/exit return or `?` because `filepath` is shadowed by an in-scope declaration (HYB-E1002)".to_string(),
                                "error-returning `fn main` lowers to `filepath.Base`; rename the shadowing `filepath` declaration so `path/filepath` is accessible",
                            );
                        }
                        if self.is_fmt_shadowed() {
                            self.emit_e1002(
                                f.span,
                                "cannot declare `fn main` with error/exit return or `?` because `fmt` is shadowed by an in-scope declaration (HYB-E1002)".to_string(),
                                "error-returning `fn main` lowers to `fmt.Fprintln`; rename the shadowing `fmt` declaration so the standard `fmt` package is accessible",
                            );
                        }
                    }
                }
            } else if f.name == "init" 
                && (!f.params.is_empty() || !f.results.is_empty() || !f.type_params.is_empty())
            {
                self.diags.push(
                    Diagnostic::error(
                        "HYB-E1002",
                        self.path.clone(),
                        f.span,
                        "`fn init` must have no parameters and no return values — Go requires `func init` to have no arguments and no return values (HYB-E1002)".to_string(),
                    )
                    .with_notes(vec![
                        "declare `fn init() { ... }` without parameters or return types".to_string(),
                    ]),
                );
            }
        }
        self.check_signature_bindings(
            f.receiver.as_ref().map(|r| r.name.as_str()),
            &f.params,
            &f.results,
            f.span,
        );

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
            let ps = self.scalar_of_ty(&p.ty);
            self.set_scalar(&p.name, ps);
            let ces = self.container_elem_scalar_of_ty(&p.ty);
            self.set_container_elem_scalar(&p.name, ces);
            self.bind_set_elem(&p.name, Self::set_elem_of_ty(&p.ty));
            self.bind_type_name(&p.name, p.ty.render());
        }
        for r in &f.results {
            if let Some(rname) = &r.name {
                self.exempt(rname);
                let rk = self.kind_of_ty(&r.ty, 0);
                self.bind_kind(rname, rk);
                let rs = self.scalar_of_ty(&r.ty);
                self.set_scalar(rname, rs);
                let ces = self.container_elem_scalar_of_ty(&r.ty);
                self.set_container_elem_scalar(rname, ces);
                self.bind_type_name(rname, r.ty.render());
            }
        }
        for r in &f.results {
            if let Some(name) = &r.name {
                self.exempt(name);
            }
        }
        let result_scalars: Vec<Option<&'static str>> =
            f.results.iter().map(|r| self.scalar_of_ty(&r.ty)).collect();
        let result_type_names: Vec<String> = f.results.iter().map(|r| r.ty.render()).collect();
        let outer_rt = std::mem::replace(&mut self.fn_result_type_names, result_type_names);
        let outer_scalars = std::mem::replace(&mut self.fn_result_scalars, result_scalars);
        let outer = std::mem::replace(&mut self.fn_results, result_kinds);
        let outer_named = std::mem::replace(&mut self.fn_results_all_named, all_named);
        let outer_err = std::mem::replace(&mut self.fn_results_error_tail, error_tail);
        let mut tp: HashSet<String> = f.type_params.iter().map(|p| p.name.clone()).collect();
        if let Some(r) = &f.receiver {
            if let TypeExpr::Named { name, .. } = &r.ty {
                if let Some(open) = name.find('[') {
                    if name.ends_with(']') {
                        for arg in split_top_level_type_args(&name[open + 1..name.len() - 1]) {
                            tp.insert(arg.to_string());
                        }
                    }
                }
            }
        }
        let outer_tp = std::mem::replace(&mut self.fn_type_params, tp);
        let outer_loop = std::mem::replace(&mut self.loop_depth, 0);
        let outer_select = std::mem::replace(&mut self.select_depth, 0);
        self.block_stmts(body);
        self.loop_depth = outer_loop;
        self.select_depth = outer_select;
        self.fn_results = outer;
        self.fn_results_all_named = outer_named;
        self.fn_results_error_tail = outer_err;
        self.fn_result_scalars = outer_scalars;
        self.fn_result_type_names = outer_rt;
        self.fn_type_params = outer_tp;
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
        if (l.names.len() == 1 && l.values.len() == 1) || l.values.len() > 1 {
            for v in &l.values {
                self.check_single_value_position(v, "a binding", /*binding_position=*/ true);
            }
        }
        // Binding-count guards: a declaration whose value count cannot
        // match its names fails go build with `assignment mismatch`.
        if l.values.len() > 1 && l.names.len() != l.values.len() {
            self.emit_e1002(
                l.span,
                format!(
                    "assignment mismatch: {} variables but {} values (HYB-E1002)",
                    l.names.len(),
                    l.values.len()
                ),
                "a parallel declaration binds positionally; the name and value counts must match",
            );
        }
        if l.names.len() > 1
            && l.values.len() == 1
            && is_definitely_single_value(&l.values[0])
        {
            self.emit_e1002(
                l.span,
                format!(
                    "assignment mismatch: {} variables but the initializer yields exactly 1 (HYB-E1002)",
                    l.names.len()
                ),
                "call, receive, and index sources are checked by the source-count rules; a provably single value binds one name",
            );
        }
        // A typed declaration must accept its initializers — Go refuses
        // `cannot use ... as ... value in variable declaration`.
        if let Some(t) = &l.ty {
            let want = self.kind_of_ty(t, 0);
            if l.values.len() == 1 && l.names.len() == 1 {
                let vk = self.kind_of_expr(&l.values[0]);
                let int_float_conflict = want == OpKind::Float
                    && vk == OpKind::Int
                    && !self.is_untyped_int_const(&l.values[0]);
                // #2770: `var h float64 = f` where `f` is a `float32`.
                let scalar_mismatch = matches!(
                    (self.scalar_of_ty(t), self.typed_value_scalar(&l.values[0])),
                    (Some(w), Some(v)) if w != v
                );
                // #2771: `var c [4]int = a` where `a` is a `[3]int`.
                if kinds_conflict(&want, &vk)
                    || int_float_conflict
                    || scalar_mismatch
                    || self.sized_array_names_differ(
                        Some(t.render().as_str()),
                        self.typed_value_type_name(&l.values[0]).as_deref(),
                    )
                {
                    self.emit_e1002(
                        l.span,
                        format!(
                            "cannot initialize `{}` (type `{}`) with a value of type `{}` — Go refuses `cannot use ... as {} value in variable declaration` (HYB-E1002)",
                            l.names[0],
                            want.label(),
                            vk.label(),
                            want.label()
                        ),
                        "the declared type is fixed; write an explicit conversion when a conversion is intended",
                    );
                }
            } else if l.values.len() > 1 && l.values.len() == l.names.len() {
                for (name, v) in l.names.iter().zip(l.values.iter()) {
                    let vk = self.kind_of_expr(v);
                    let int_float_conflict = want == OpKind::Float
                        && vk == OpKind::Int
                        && !self.is_untyped_int_const(v);
                    // #2770: concrete numeric type differing under one `OpKind`.
                    let scalar_mismatch = matches!(
                        (self.scalar_of_ty(t), self.typed_value_scalar(v)),
                        (Some(w), Some(g)) if w != g
                    );
                    // #2771: sized-array length must match the declared type.
                    if kinds_conflict(&want, &vk)
                        || int_float_conflict
                        || scalar_mismatch
                        || self.sized_array_names_differ(
                            Some(t.render().as_str()),
                            self.typed_value_type_name(v).as_deref(),
                        )
                    {
                        self.emit_e1002(
                            l.span,
                            format!(
                                "cannot initialize `{}` (type `{}`) with a value of type `{}` — Go refuses `cannot use ... as {} value in variable declaration` (HYB-E1002)",
                                name,
                                want.label(),
                                vk.label(),
                                want.label()
                            ),
                            "the declared type is fixed; write an explicit conversion when a conversion is intended",
                        );
                    }
                }
            }
        }
        // Composed resolution (pr-430 x landed pr-475): #475's track_read
        // set (Define|Let|Var|Const) drives which locals get unused-read
        // tracking; #430's operand-kind inference runs for EVERY name so
        // `kind_of_expr` resolves binding-shaped operands (issue #425).
        let kinds = self.infer_local_kinds(l);
        let scalars = self.infer_local_scalars(l);
        // #2858: track container element numeric scalar for slices/arrays/maps
        let container_elem_scalars: Vec<Option<&'static str>> = match &l.ty {
            Some(t) => vec![self.container_elem_scalar_of_ty(t); l.names.len()],
            None if l.values.len() == l.names.len() => {
                l.values.iter().map(|v| self.container_elem_scalar_of_expr(v)).collect()
            }
            _ => vec![None; l.names.len()],
        };
        let set_elems: Vec<Option<String>> = (0..l.names.len())
            .map(|i| match (&l.ty, l.values.len() == l.names.len()) {
                (Some(t), _) => Self::set_elem_of_ty(t),
                (None, true) => self.set_elem_of_expr(&l.values[i]),
                _ => None,
            })
            .collect();
        if let Some(t) = &l.ty {
            let tname = t.render();
            for name in &l.names {
                self.bind_type_name(name, tname.clone());
            }
        } else if l.values.len() == l.names.len() {
            for (name, val) in l.names.iter().zip(&l.values) {
                // `a := [2]int{..}` / `b := a`: an inferred sized array keeps
                // its length so `a = [3]int{..}`, `take(a)` etc. are compared.
                if let Some(tn) = self.typed_value_type_name(val).filter(|t| is_sized_array_spelling(t)) {
                    self.bind_type_name(name, tn);
                } else if let Some(elem) = self.slice_element_type_name(val) {
                    self.bind_type_name(name, format!("[]{}", elem));
                } else if let Some(tp) = self.expr_type_param(val) {
                    self.bind_type_name(name, tp);
                }
            }
        }
        for ((((name, kind), scalar), set_elem), container_elem_scalar) in
            l.names.iter().zip(kinds).zip(scalars).zip(set_elems).zip(container_elem_scalars)
        {
            // #1031: a same-scope `:=` re-define keeps the EXISTING variable
            // (and its type) in Go, so only a fresh binding takes the
            // initializer's type; a re-define becomes unknown (fail-open).
            let fresh = !self
                .scalar_scopes
                .last()
                .is_some_and(|top| top.contains_key(name.as_str()));
            self.bind_kind(name, kind);
            if fresh || l.ty.is_some() {
                self.set_scalar(name, scalar);
                self.set_container_elem_scalar(name, container_elem_scalar);
            }
            self.bind_set_elem(name, set_elem);
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
                is_untyped: matches!(l.kind, LocalKind::Const) && l.ty.is_none(),
            });
            top.insert(name.clone(), Some(id));
        }
    }

    fn check_const_decl(&mut self, cd: &hy_syntax::ConstDecl) {
        let Some(t) = &cd.ty else {
            return;
        };
        let want = self.kind_of_ty(t, 0);
        let vk = self.kind_of_expr(&cd.value);
        // `const arr [2]int = [10, 20]` uses bracketed array-literal shorthand
        // (`Expr::SliceLit { elem: None, .. }`) where `check_local_array_lit_len_match`
        // validates the element count.
        let unheaded_array_shorthand = matches!(
            (&want, &vk),
            (OpKind::Array(a), OpKind::Slice(b))
                if !kinds_conflict(a, b)
                    && matches!(unwrap_parens(&cd.value), Expr::SliceLit { elem: None, .. })
        );
        let untyped_integral_float_to_int = want == OpKind::Int
            && vk == OpKind::Float
            && self.const_expr_scalar(&cd.value).is_none()
            && self
                .ck
                .const_float_fold(&cd.value, None)
                .is_some_and(|v| v.is_finite() && v.fract() == 0.0);
        if unheaded_array_shorthand || untyped_integral_float_to_int {
            return;
        }
        let int_float_conflict = want == OpKind::Float
            && vk == OpKind::Int
            && !self.is_untyped_int_const(&cd.value);
        let scalar_mismatch = matches!(
            (self.scalar_of_ty(t), self.const_expr_scalar(&cd.value)),
            (Some(w), Some(v)) if w != v
        );
        if kinds_conflict(&want, &vk)
            || int_float_conflict
            || scalar_mismatch
            || self.sized_array_names_differ(
                Some(t.render().as_str()),
                self.const_expr_type_name(&cd.value).as_deref(),
            )
        {
            self.emit_e1002(
                cd.span,
                format!(
                    "cannot initialize `{}` (type `{}`) with a value of type `{}` — Go refuses `cannot use ... as {} value in constant declaration` (HYB-E1002)",
                    cd.name,
                    want.label(),
                    vk.label(),
                    want.label()
                ),
                "the declared type is fixed; write an explicit conversion when a conversion is intended",
            );
        }
    }

    fn const_expr_scalar(&self, e: &Expr) -> Option<&'static str> {
        if let Some(s) = self.typed_value_scalar(e) {
            return Some(s);
        }
        match unwrap_parens(e) {
            Expr::Ident { name, .. } => self.ck.file.decls.iter().find_map(|d| match d {
                Decl::Const(other) if other.name == *name => {
                    other.ty.as_ref().and_then(|t| self.scalar_of_ty(t))
                }
                _ => None,
            }),
            Expr::Unary {
                op: UnOp::Neg,
                operand,
                ..
            } => self.const_expr_scalar(operand),
            Expr::Binary { op, lhs, rhs, .. } => match op {
                BinOp::Add
                | BinOp::Sub
                | BinOp::Mul
                | BinOp::Div
                | BinOp::Mod
                | BinOp::BitOr
                | BinOp::BitXor
                | BinOp::BitAnd => self
                    .const_expr_scalar(lhs)
                    .or_else(|| self.const_expr_scalar(rhs)),
                BinOp::Shl | BinOp::Shr => self.const_expr_scalar(lhs),
                _ => None,
            },
            _ => None,
        }
    }

    fn const_expr_type_name(&self, e: &Expr) -> Option<String> {
        if let Some(n) = self.typed_value_type_name(e) {
            return Some(n);
        }
        match unwrap_parens(e) {
            Expr::Ident { name, .. } => self.ck.file.decls.iter().find_map(|d| match d {
                Decl::Const(other) if other.name == *name => {
                    other.ty.as_ref().map(|t| t.render())
                }
                _ => None,
            }),
            _ => None,
        }
    }

    fn infer_local_kinds(&self, l: &LocalDecl) -> Vec<OpKind> {
        let n = l.names.len();
        if let Some(ty) = &l.ty {
            return vec![self.kind_of_ty(ty, 0); n];
        }
        if n >= 2 && l.values.len() == 1 {
            let mut mv = self.multi_value_kinds(&l.values[0]);
            mv.resize(n, OpKind::Unknown);
            return mv;
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
                OpKind::Unknown => vec![OpKind::Unknown, OpKind::Bool],
                _ => vec![OpKind::Unknown, OpKind::Unknown],
            },
            Expr::Receive { chan, .. } => match self.kind_of_expr(chan) {
                OpKind::Chan(elem) => vec![*elem, OpKind::Bool],
                _ => vec![OpKind::Unknown, OpKind::Bool],
            },
            Expr::Call { callee, args, .. } => {
                if let Some(res) = self.call_result_kinds(callee, args) {
                    if res.len() >= 2 {
                        return res;
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
                    // #2199: a store whose target is a TYPE NAME cannot build.
                    // The annotated-store spelling `n T = v` parses as the
                    // expression statement `n` followed by a store to `T`, and
                    // Go refuses the second line (`T (type) is not an
                    // expression`). Same class as #778, one position over.
                    if let PlaceKind::Ident(name) = &t.kind {
                        self.check_type_name_in_value_position(name, t.span);
                    }
                    self.place(t, *op);
                }
                for v in values {
                    self.expr(v);
                }
                // #682 U8: `a = v()` where `v` produces no value. The
                // multi-name form (`a, b = f()`) is a legal multi-value
                // assignment, so only the single-target/single-value shape is
                // checked here.
                if (targets.len() == 1 && values.len() == 1) || values.len() > 1 {
                    for v in values {
                        self.check_single_value_position(v, "an assignment", /*binding_position=*/ true);
                    }
                }
                self.check_assign_operands(targets, *op, values, *span);
            }
            Stmt::Expr { expr, .. } => self.expr(expr),
            Stmt::Send { chan, value, .. } => {
                self.expr(chan);
                // #887: the sent value is a single-value position. A void call
                // or a multi-value call here passed the checker and emitted
                // `ch <- noop()` / `ch <- pair()`, which Go rejects.
                self.check_single_value_position(value, "a channel send", false);
                self.expr(value);
                self.check_send_elem(chan, value);
                // #2817: the sent value must be assignable to the channel's
                // element type; Go refuses `ch <- f` for mismatched widths.
                self.check_send_assignability(chan, value);
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
                        self.check_single_value_position(expr, "a for-range expression", false);
                        self.expr(expr);
                        let iter_kinds = self.iterate_var_kinds(expr, vars.len());
                        // #3065 / SPEC LX13: a string ranges over runes, so
                        // the element binding (the last var) is exactly
                        // `rune` (= int32), not an untyped int; the width
                        // checks then catch `var b byte = c` and `n + c`.
                        let str_elem = matches!(self.kind_of_expr(expr), OpKind::Str)
                            .then(|| vars.len().saturating_sub(1));
                        // Unread iterate bindings are lowered to `_` at
                        // emission (issue #139); write-only ones still
                        // reject as unused (PR #536).
                        for (vi, (v, k)) in vars.iter().zip(iter_kinds).enumerate() {
                            self.define_elidable(v, *span);
                            // #1480: a struct/array element binding is a
                            // per-iteration copy at emission (`for _, x :=
                            // range xs`), so every write through it is
                            // silently dropped. Record the binding so the
                            // write places and the pointer-receiver call
                            // below reject instead of certifying the pass.
                            if v != "_"
                                && matches!(k, OpKind::Struct(_) | OpKind::Array(_))
                            {
                                if let Some(top) = self.copy_var_scopes.last_mut() {
                                    top.insert(v.clone());
                                }
                            }
                            self.bind_kind(v, k);
                            if str_elem == Some(vi) {
                                self.set_scalar(v, Some("int32"));
                            }
                        }
                        // #2858: record element scalar for iterate variables
                        if let Some(iter_elem_scalar) = self.container_elem_scalar_of_expr(expr) {
                            if vars.len() == 1 {
                                if vars[0] != "_" {
                                    self.set_scalar(&vars[0], Some(iter_elem_scalar));
                                }
                            } else if vars.len() >= 2 {
                                if vars[0] != "_" && matches!(self.kind_of_expr(expr), OpKind::Slice(_) | OpKind::Array(_) | OpKind::Str) {
                                    self.set_scalar(&vars[0], Some("int"));
                                }
                                if vars[1] != "_" {
                                    self.set_scalar(&vars[1], Some(iter_elem_scalar));
                                }
                            }
                        } else if vars.len() >= 2 && vars[0] != "_" && matches!(self.kind_of_expr(expr), OpKind::Slice(_) | OpKind::Array(_) | OpKind::Str) {
                            self.set_scalar(&vars[0], Some("int"));
                        }
                    }
                    ForKind::Infinite => {}
                }
                self.block_scoped(body);
                self.loop_depth -= 1;
                self.pop_scope();
            }
            Stmt::Return { values, span } => {
                for (i, v) in values.iter().enumerate() {
                    // SPEC E3 zero-value return placeholder (`return _, err`):
                    // in a function whose results end in `error`, `hy-emit`
                    // lowers `_` in a return slot (`i < fn_results.len()`) to
                    // the result type's zero value (`examples/09-go-imports-hybrid`).
                    if self.fn_results_error_tail
                        && i < self.fn_results.len()
                        && matches!(v, Expr::Ident { name, .. } if name == "_")
                    {
                        continue;
                    }
                    self.expr(v);
                }
                self.check_return(values, *span);
            }
            Stmt::Task { callee, args, span } => {
                self.expr(callee);
                // #887: a spawn argument sits in a single-value position
                // exactly like a call argument does. Without this,
                // `task worker(noop())` (void call) and
                // `task worker(pair())` (two values) passed the checker
                // and emitted Go that cannot build.
                //
                // The #682 U7 exception applies to a spawn exactly as it does
                // to a call: a lone multi-value call IS the whole argument
                // list when the callee takes that many parameters, so
                // `go w(pair())` builds for a 2-param `w`. Rejecting it here
                // made the checker refuse legal Go. When the shape is taken,
                // the arity is what decides, and that is checked below.
                if self.multi_value_arg_count(args).is_none() {
                    for a in args {
                        self.check_single_value_position(&a.value, "a spawn argument", false);
                    }
                } else {
                    self.check_multi_value_arg_arity(callee, args, *span);
                }
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
                    Expr::FnLit { params, .. } => {
                        self.check_fn_lit_call(params, args, callee, *span);
                    }
                    _ => {}
                }
            }
            Stmt::Managed { body, .. } => {
                self.managed_depth += 1;
                self.block_scoped(body);
                self.managed_depth -= 1;
            }
            Stmt::Match { subject, arms, .. } => {
                self.expr(subject);
                let subject_kind = self.kind_of_expr(subject);
                for arm in arms {
                    self.push_scope();
                    match &arm.pattern {
                        MatchPattern::Path { segments, binding } => {
                            if let Some(root) = segments.first() {
                                self.mention(root);
                                if root == "nil" && is_non_nilable(&subject_kind) {
                                    self.emit_e1002(
                                        subject.span(),
                                        format!(
                                            "cannot use match pattern `nil` on subject of non-nilable type `{}` — Go refuses `invalid case nil in switch on ... (mismatched types untyped nil and {})` (HYB-E1002)",
                                            subject_kind.label(),
                                            subject_kind.label()
                                        ),
                                        "`nil` match pattern is only valid on nilable subjects (pointers, interfaces, channels, maps, slices, options)",
                                    );
                                }
                            }
                            if let Some(b) = binding {
                                self.define_elidable(b, subject.span());
                                // #2915: payload binding `some(v)` inherits the element type of `option[T]`.
                                let bind_k = if segments.first().map(|s| s.as_str()) == Some("some") {
                                    if let OpKind::Option(elem) = &subject_kind {
                                        (**elem).clone()
                                    } else {
                                        OpKind::Unknown
                                    }
                                } else {
                                    OpKind::Unknown
                                };
                                self.bind_kind(b, bind_k);
                            }
                        }
                        MatchPattern::Lit(lit) => {
                            self.check_match_lit_pattern(subject, &subject_kind, lit, subject.span());
                        }
                        MatchPattern::Wildcard => {}
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
                    // #887: a channel send in a select arm is also a single-value
                    // position.
                    self.check_single_value_position(value, "a channel send", false);
                            self.expr(chan);
                            self.expr(value);
                            self.check_send_elem(chan, value);
                            // #2817: same assignability rule inside `select`.
                            self.check_send_assignability(chan, value);
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

    // #1112-family: a send (statement or select arm) must carry a value
    // assignable to the channel's element type — go build refuses
    // `cannot use ... as <elem> value in send`. Fail open when the channel's
    // element type is unknown.
    fn check_send_elem(&mut self, chan: &Expr, value: &Expr) {
        // O6.1: inside a `managed` block the sendability check is hatched
        // (the block's contract already accepts unchecked sharing).
        if self.managed_depth > 0 {
            return;
        }
        let ck = self.kind_of_expr(chan);
        let vk = self.kind_of_expr(value);
        if let OpKind::Chan(elem) = ck {
            let elem = *elem;
            let int_float_conflict =
                elem == OpKind::Float && vk == OpKind::Int && !self.is_untyped_int_const(value);
            if kinds_conflict(&elem, &vk) || int_float_conflict {
                self.emit_e1002(
                    value.span(),
                    format!(
                        "cannot send a value of type `{}` on a `chan {}` — Go refuses `cannot use ... as {} value in send` (HYB-E1002)",
                        vk.label(),
                        elem.label(),
                        elem.label()
                    ),
                    "a channel's element type is fixed at make(); write an explicit conversion when a conversion is intended",
                );
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
                // SPEC LX13: single-var `for k in m` binds the KEY (#2945).
                if n_vars >= 2 {
                    vec![*k, *v]
                } else {
                    vec![*k]
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
            PlaceKind::Index { base, .. } => {
                let bk = self.kind_of_place(base);
                if matches!(bk, OpKind::Str) {
                    self.emit_e1002(
                        p.span,
                        format!(
                            "cannot assign to string index `{}` — strings are immutable (HYB-E1002)",
                            p.render()
                        ),
                        "strings in Go are immutable; string elements cannot be assigned or updated",
                    );
                }
                self.check_range_copy_write(p);
                self.place_read(p);
            }
            PlaceKind::Field { base, name } => {
                let bk = self.kind_of_place(base);
                self.check_field_selector_kind(&base.render(), name, &bk, p.span);
                self.check_range_copy_write(p);
                self.place_read(p);
            }
            PlaceKind::AddrOf { .. } => {
                self.check_range_copy_write(p);
                self.place_read(p);
            }
        }
    }

    /// #1480: is `name` a `for ... in` binding over copy-semantics storage?
    fn is_copy_semantic_range_var(&self, name: &str) -> bool {
        self.copy_var_scopes
            .iter()
            .rev()
            .any(|layer| layer.contains(name))
    }

    /// #1480: root name of a write place that provably lands in a
    /// per-iteration range-element copy. The base chain is walked down to
    /// the root; every step must stay inside copy-semantics storage:
    /// * stepping through a pointer field or an unknown kind redirects into
    ///   (or may redirect into) shared storage — fail open, no diagnostic;
    /// * `slice`/`map`/`string` element writes reach the shared collection
    ///   and stay legal; an `array` element of the copy is itself a copy.
    fn copy_place_root(&self, p: &Place) -> Option<String> {
        match &p.kind {
            PlaceKind::Ident(n) => {
                if self.is_copy_semantic_range_var(n) {
                    Some(n.clone())
                } else {
                    None
                }
            }
            PlaceKind::Field { base, .. } => match self.kind_of_place(base) {
                OpKind::Unknown | OpKind::Pointer(_) => None,
                _ => self.copy_place_root(base),
            },
            PlaceKind::Index { base, .. } => match self.kind_of_place(base) {
                OpKind::Unknown | OpKind::Pointer(_) | OpKind::Slice(_) | OpKind::Map(..) => None,
                OpKind::Str => None,
                _ => self.copy_place_root(base),
            },
            PlaceKind::AddrOf { .. } => None,
        }
    }

    /// #1480: `x.f = v`, `x.f += v`, `x[i] = v` … where `x` is a
    /// struct/array element of a `for x in xs` loop are silently dropped at
    /// runtime (the binding is a per-iteration copy). The pass must not
    /// certify them.
    fn check_range_copy_write(&mut self, p: &Place) {
        let Some(root) = self.copy_place_root(p) else {
            return;
        };
        self.emit_e1002(
            p.span,
            format!(
                "cannot mutate `{}` through range element `{root}` — `{root}` is a per-iteration copy of the element, so the write is silently dropped (HYB-E1002)",
                p.render()
            ),
            "assign through the collection instead: `for i in 0..len(xs) { xs[i].… }`, or iterate a slice of pointers and mutate through the pointer",
        );
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
                // Issue #999: pass the base as an expression so the constant
                // index-bounds rule (fixed-size arrays, constant strings) can
                // resolve the base's declared length on assignment places —
                // the read path (`a[i]` in expression position) already does;
                // the `None` here silently disabled the array-length lookup
                // for `a[300] = 1` on a var-declared array.
                let base_expr = place_as_expr(base);
                self.check_index_kind(&base.render(), base_expr.as_ref(), &bk, index, &ik, p.span);
            }
            PlaceKind::Field { base, .. } => self.place_read(base),
            PlaceKind::AddrOf { base } => self.place_read(base),
        }
    }

    // -- expressions and types ----------------------------------------------

    fn expr(&mut self, e: &Expr) {
        match e {
            Expr::Ident { name, span } => {
                // #2710: the blank identifier `_` discards a value; it is not
                // itself a value. Go refuses to read it (`cannot use _ as value
                // or type`). This arm is only reached for VALUE positions —
                // assignment/declaration targets go through `place`/`local`,
                // which never route the target identifier through `expr`, so
                // `_ = v` and `x, _ := f()` stay legal.
                if name == "_" {
                    self.emit_e1002(
                        *span,
                        "cannot use `_` as a value — Go refuses `cannot use _ as value or type` (HYB-E1002)".to_string(),
                        "the blank identifier `_` may only appear on the left-hand side of an assignment or declaration, to discard a value",
                    );
                }
                self.read(name);
                // #778: a value position, so a type name here cannot build.
                self.check_type_name_in_value_position(name, *span);
            }
            Expr::Lit { .. } => {}
            Expr::Interp { parts, span, .. } => {
                for p in parts {
                    if let InterpPart::Expr(x) = p {
                        self.expr(x);
                    }
                }
                // #3568 — an interpolation with an expression part lowers to
                // `fmt.Sprintf(...)` (with implicit `import "fmt"`). If `fmt` is bound
                // in an enclosing lexical/generic scope or declared at package scope,
                // Go captures or collides with `fmt` (`fmt.Sprintf undefined` or
                // `fmt redeclared in this block`).
                if parts.iter().any(|p| matches!(p, InterpPart::Expr(_))) && self.is_fmt_shadowed() {
                    self.emit_e1002(
                        *span,
                        "string interpolation requires package `fmt`, which is shadowed by an in-scope `fmt` declaration — Go refuses `fmt.Sprintf` (HYB-E1002)".to_string(),
                        "rename the `fmt` binding or declaration so emitter-generated `fmt.Sprintf(...)` resolves to Go's `fmt` package",
                    );
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
            Expr::AddrOf { operand, span, .. } => {
                self.expr(operand);
                self.check_addr_of_operand(operand, *span);
            }
            Expr::Move { operand, .. } => self.expr(operand),
            Expr::Try { operand, .. } => self.expr(operand),
            Expr::Receive { chan, .. } => {
                self.check_single_value_position(chan, "a receive operation", false);
                self.expr(chan);
            }
            Expr::Index {
                base, index, span, ..
            } => {
                self.check_single_value_position(base, "an index operand", false);
                self.expr(base);
                self.expr(index);
                let bk = self.kind_of_expr(base);
                let ik = self.kind_of_expr(index);
                self.check_index_kind(&base.render(), Some(base.as_ref()), &bk, index, &ik, *span);
            }
            Expr::Slice {
                base, lo, hi, span, ..
            } => {
                self.check_single_value_position(base, "a slice operand", false);
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
                self.check_field_selector(base, name, *span);
                // #778: a Field BASE is a QUALIFIER position, not a value
                // position, so the type-name check must not fire on it. Two
                // legal spellings put a type name there:
                //   * `color.red`      — an enum variant
                //   * `greeting.build` — a method call on the type
                // (and `fmt.println` is a package root, which is not a type
                // and never reached this check anyway). A bare `x := colour`
                // IS a value position and still rejects, pinned in the
                // fixture. An identifier base is read for used-marking; a
                // deeper base (a call result, an index) recurses normally.
                match unwrap_parens(base) {
                    Expr::Ident { name, .. } => self.read(name),
                    other => self.expr(other),
                }
            }
            Expr::Call {
                callee, args, span, ..
            } => {
                // #778: a plain identifier CALLEE is a call position, not a
                // value position — `holder(x)` is a legal conversion and
                // `int(3.7)` is a legal built-in one, so the type-name check
                // must not fire there. `read` still runs, because an unused
                // local closure called as `f()` has been read.
                match unwrap_parens(callee) {
                    Expr::Ident { name, .. } => self.read(name),
                    other => self.expr(other),
                }
                // `new(T)` and `make(T, len, cap)` take a TYPE as their FIRST
                // argument, so that position is a type position and the
                // #778 value-position check must not fire there — `new(bag)`
                // and `make(map[string]int)` are both legal. The remaining
                // `make` arguments (len, cap) ARE values and still walk.
                let type_first_arg = matches!(
                    unwrap_parens(callee),
                    Expr::Ident { name, .. } if name == "new" || name == "make"
                );
                if type_first_arg {
                    if let Some(first_arg) = args.first() {
                        // #3248: the expression walk below skips this argument,
                        // so the type it carries must still go through the
                        // declared-position walker (`ty()`) explicitly — that
                        // is where map-key comparability is enforced. Without
                        // this, `make(chan map[[]int]int, 1)` slipped an
                        // unhashable map key past the checker inside a
                        // chan/slice/array wrapper or type alias.
                        match unwrap_parens(&first_arg.value) {
                            Expr::Index { base, index, span } if matches!(unwrap_parens(base), Expr::Ident { name, .. } if self.lookup_kind(name).is_none() && !self.file_consts.contains_key(name) && !self.ck.fns.contains_key(name)) => {
                                self.check_named_type_instantiation(&format!("{}[{}]", base.render(), index.render()), *span);
                            }
                            _ => {}
                        }
                        // #3561: validate the type expression in new(T)/make(T, ...).
                        // Union of #3561 (recursive instantiation walker) with #3355
                        // (declared-position walks: alias ty, StructLit/SliceLit/array
                        // elem ty, MapLit key comparability) — each shape walks once.
                        self.check_type_arg_instantiation(&first_arg.value);
                    }
                }
                for (i, a) in args.iter().enumerate() {
                    if type_first_arg && i == 0 {
                        continue;
                    }
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
                self.check_set_method_arg(callee, args);
                self.check_option_method_arg(callee, args);
            }
            Expr::StructLit { ty, fields, span } => {
                if let Some(t) = ty {
                    self.ty(t);
                    self.check_value_position_type_name(t);
                }
                for f in fields {
                    self.expr(&f.value);
                }
                self.check_struct_literal(ty.as_ref(), fields, *span);
            }
            Expr::SliceLit { elem, items, .. } => {
                if let Some(t) = elem {
                    self.ty(t);
                    self.check_value_position_type_name(t);
                }
                let in_slot = self.elided_slots.contains(&(e as *const Expr));
                if elem.is_some() || in_slot {
                    for i in items {
                        self.elided_slots.insert(unwrap_parens(i) as *const Expr);
                    }
                }
                for i in items {
                    self.expr(i);
                }
                if let Some((_, inner_elem, _)) = e.as_array_lit() {
                    self.check_array_lit_items(inner_elem, items);
                } else if let Some(t) = elem {
                    self.check_slice_lit_items(t, items);
                } else if !in_slot {
                    self.check_untyped_slice_lit_items(items);
                }
            }
            Expr::MapLit {
                key,
                value,
                entries,
                span,
                ..
            } => {
                self.check_map_key_comparable(key, *span);
                self.ty(key);
                self.ty(value);
                self.check_value_position_type_name(key);
                self.check_value_position_type_name(value);
                for (k, v) in entries {
                    self.elided_slots.insert(unwrap_parens(k) as *const Expr);
                    self.elided_slots.insert(unwrap_parens(v) as *const Expr);
                }
                for (k, v) in entries {
                    self.expr(k);
                    self.expr(v);
                }
                self.check_map_lit_entries(key, value, entries);
            }
            Expr::FnLit {
                params,
                results,
                body,
                span,
            } => {
                self.check_signature_bindings(None, params, results, *span);
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
                    let ps = self.scalar_of_ty(&p.ty);
                    self.set_scalar(&p.name, ps);
                    let ces = self.container_elem_scalar_of_ty(&p.ty);
                    self.set_container_elem_scalar(&p.name, ces);
                    self.bind_set_elem(&p.name, Self::set_elem_of_ty(&p.ty));
                    self.bind_type_name(&p.name, p.ty.render());
                }
                for r in results {
                    if let Some(rname) = &r.name {
                        self.exempt(rname);
                        let rk = self.kind_of_ty(&r.ty, 0);
                        self.bind_kind(rname, rk);
                        let rs = self.scalar_of_ty(&r.ty);
                        self.set_scalar(rname, rs);
                        let ces = self.container_elem_scalar_of_ty(&r.ty);
                        self.set_container_elem_scalar(rname, ces);
                        self.bind_type_name(rname, r.ty.render());
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
                let result_scalars: Vec<Option<&'static str>> =
                    results.iter().map(|r| self.scalar_of_ty(&r.ty)).collect();
                let result_type_names: Vec<String> =
                    results.iter().map(|r| r.ty.render()).collect();
                let outer_rt = std::mem::replace(&mut self.fn_result_type_names, result_type_names);
                let outer_scalars = std::mem::replace(&mut self.fn_result_scalars, result_scalars);
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
                self.fn_result_scalars = outer_scalars;
                self.fn_result_type_names = outer_rt;
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
            TypeExpr::Map { key, value, span } => {
                self.check_map_key_comparable(key, *span);
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

    fn strip_leading_bracket_group(s: &str) -> Option<(&str, &str)> {
        if !s.starts_with('[') {
            return None;
        }
        let mut depth = 0usize;
        for (i, ch) in s.char_indices() {
            match ch {
                '[' => depth += 1,
                ']' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some((&s[1..i], &s[i + 1..]));
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn substitute_type_str(s: &str, subst: &HashMap<String, String>) -> String {
        let s = s.trim();
        if let Some(repl) = subst.get(s) {
            return repl.clone();
        }
        if let Some(rest) = s.strip_prefix('*') {
            return format!("*{}", Self::substitute_type_str(rest, subst));
        }
        if let Some(rest) = s.strip_prefix("[]") {
            return format!("[]{}", Self::substitute_type_str(rest, subst));
        }
        for pfx in ["<-chan ", "chan<- ", "chan "] {
            if let Some(rest) = s.strip_prefix(pfx) {
                return format!("{}{}", pfx, Self::substitute_type_str(rest, subst));
            }
        }
        if s.starts_with('[') {
            if let Some((inside, rest)) = Self::strip_leading_bracket_group(s) {
                if !rest.trim().is_empty() {
                    return format!("[{}]{}", inside, Self::substitute_type_str(rest, subst));
                }
            }
        }
        if let Some(after_map) = s.strip_prefix("map") {
            if let Some((k, v)) = Self::strip_leading_bracket_group(after_map) {
                if !v.trim().is_empty() {
                    return format!(
                        "map[{}]{}",
                        Self::substitute_type_str(k, subst),
                        Self::substitute_type_str(v, subst)
                    );
                }
            }
        }
        if let Some(open) = s.find('[') {
            if s.ends_with(']') {
                let head = &s[..open];
                let inner = &s[open + 1..s.len() - 1];
                let args: Vec<String> = split_top_level_type_args(inner)
                    .into_iter()
                    .map(|a| Self::substitute_type_str(a, subst))
                    .collect();
                return format!("{}[{}]", head, args.join(", "));
            }
        }
        s.to_string()
    }

    fn incomparable_type_noun(&self, ty_str: &str) -> Option<&'static str> {
        self.incomparable_type_noun_depth(ty_str, 0)
    }

    fn incomparable_type_noun_depth(&self, ty_str: &str, depth: usize) -> Option<&'static str> {
        if depth > 16 {
            return None;
        }
        let trimmed = ty_str.trim();
        if trimmed.starts_with('*')
            || trimmed.starts_with("chan ")
            || trimmed.starts_with("<-chan ")
            || trimmed.starts_with("chan<- ")
        {
            return None;
        }
        if trimmed.starts_with("[]") {
            return Some("slice");
        }
        if trimmed.starts_with('[') {
            if let Some((inside, rest)) = Self::strip_leading_bracket_group(trimmed) {
                if !inside.is_empty() && !rest.trim().is_empty() {
                    return self.incomparable_type_noun_depth(rest, depth + 1);
                }
            }
        }
        if trimmed.starts_with("map[") {
            return Some("map");
        }
        if trimmed.starts_with("func(") || trimmed.starts_with("fn(") || trimmed == "fn" || trimmed == "func" {
            return Some("function");
        }
        if trimmed.starts_with("set[") || trimmed == "set" {
            return Some("set");
        }
        if trimmed.starts_with("option[") || trimmed == "option" {
            return Some("option");
        }
        if let Some(alias_ty) = self.ck.type_aliases.get(trimmed) {
            return self.incomparable_type_noun_depth(&alias_ty.render(), depth + 1);
        }
        for d in &self.ck.file.decls {
            if let hy_syntax::Decl::Enum(e) = d {
                if e.name == trimmed && e.variants.iter().any(|v| v.payload.is_some()) {
                    return Some("data-carrying enum");
                }
            }
        }
        let base = Checker::base_named_type_str(trimmed);
        if let Some(fields) = self.ck.structs.get(base) {
            let subst = self.struct_type_arg_map(trimmed);
            for (_, fty) in fields {
                let render = if subst.is_empty() {
                    fty.render()
                } else {
                    Self::substitute_type_str(&fty.render(), &subst)
                };
                if let Some(n) = self.incomparable_type_noun_depth(&render, depth + 1) {
                    return Some(n);
                }
            }
        }
        None
    }

    fn check_map_key_comparable(&mut self, key: &TypeExpr, span: Span) {
        let noun = match key {
            TypeExpr::Slice { .. } => Some("slice"),
            TypeExpr::Map { .. } => Some("map"),
            TypeExpr::Fn { .. } => Some("function"),
            _ => self.incomparable_type_noun(&key.render()),
        };
        if let Some(noun) = noun {
            let key_ty_str = key.render();
            self.emit_e1002(
                span,
                format!(
                    "invalid map key type `{}`: {} is not comparable (HYB-E1002)",
                    key_ty_str, noun
                ),
                "map key types must be comparable (primitives, pointers, strings, or structs without slice/map fields)",
            );
        }
    }

    /// #3561: validate the type expression in `new(T)` or `make(T, ...)`.
    /// Union resolution with #3355: #3561's recursive instantiation walker plus
    /// #3355's declared-position walks (alias ty, StructLit/SliceLit/array elem
    /// ty, MapLit key comparability) — each shape walks exactly once.
    fn check_type_arg_instantiation(&mut self, e: &Expr) {
        match unwrap_parens(e) {
            Expr::Ident { name, span } => {
                self.check_named_type_instantiation(name, *span);
                if let Some(alias_ty) = self.ck.type_aliases.get(name.as_str()) {
                    self.ty(alias_ty);
                }
            }
            // The composite `head[args]` is instantiated by the caller-side match
            // arm; recursing into `base` alone would re-check a bare generic head
            // and false-positive arity/shadow diagnostics (#3684 accept pins).
            // Walk the type arguments only.
            Expr::Index { index, .. } => {
                self.check_type_arg_instantiation(index);
            }
            Expr::StructLit { ty: Some(t), .. } => self.ty(t),
            Expr::SliceLit { elem: Some(t), .. } => self.ty(t),
            Expr::MapLit {
                key, value, span, ..
            } => {
                self.check_map_key_comparable(key, *span);
                self.ty(key);
                self.ty(value);
            }
            other => {
                if let Some((_, elem, _)) = other.as_array_lit() {
                    self.ty(elem);
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
        if name.starts_with('[') {
            if let Some((inside, rest)) = Self::strip_leading_bracket_group(name) {
                if !inside.is_empty() && !rest.trim().is_empty() {
                    self.check_named_type_instantiation(rest, span);
                    return;
                }
            }
        }
        if let Some(after_map) = name.strip_prefix("map") {
            if let Some((k, v)) = Self::strip_leading_bracket_group(after_map) {
                if !v.trim().is_empty() {
                    let k_trimmed = k.trim();
                    if let Some(noun) = self.incomparable_type_noun(k_trimmed) {
                        self.emit_e1002(
                            span,
                            format!(
                                "invalid map key type `{}`: {} is not comparable (HYB-E1002)",
                                k_trimmed, noun
                            ),
                            "map key types must be comparable (primitives, pointers, strings, or structs without slice/map fields)",
                        );
                    }
                    self.check_named_type_instantiation(k, span);
                    self.check_named_type_instantiation(v, span);
                    return;
                }
            }
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
        // #3561: if `head` is shadowed by a value binding in any enclosing scope,
        // using it in a type position causes Go to resolve it to the shadowing
        // variable, failing compilation with `<ident> is not a type` (Row 13 BUILDABLE).
        if self.scopes.iter().rev().any(|layer| layer.contains_key(head)) {
            let is_type = self.ck.structs.contains_key(head)
                || self.ck.struct_type_params.contains_key(head)
                || self.ck.enums.contains_key(head)
                || self.ck.type_aliases.contains_key(head)
                || crate::const_init::is_conversion_type_name(head);
            if is_type {
                self.emit_e1002(
                    span,
                    format!(
                        "`{}` is shadowed by a local variable — Go refuses `{} is not a type` (HYB-E1002)",
                        head, head
                    ),
                    "a type name cannot be used where it is shadowed by a local variable or parameter",
                );
                return;
            }
        }
        if head == "set" {
            if let Some(inner) = inner_opt {
                let args = split_top_level_type_args(inner);
                if args.len() == 1 {
                    let elem_ty = args[0].trim();
                    if let Some(noun) = self.incomparable_type_noun(elem_ty) {
                        self.emit_e1002(
                            span,
                            format!(
                                "invalid set element type `{}`: {} is not comparable — `set[T]` requires comparable `T` (SPEC §2.3 T9) (HYB-E1002)",
                                elem_ty, noun
                            ),
                            "set element types must be comparable (primitives, pointers, or structs without slice/map fields)",
                        );
                    }
                }
            }
            return;
        }
        if head == "option" || head == "map" {
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

    fn param_kind(&self, p: &Param, depth: usize) -> OpKind {
        match &p.ty {
            TypeExpr::Named { name, .. } if name.is_empty() => {
                self.kind_of_named(&p.name, depth)
            }
            _ => self.kind_of_ty(&p.ty, depth),
        }
    }

    fn param_kind_with_subst(
        &self,
        p: &Param,
        subst: &HashMap<String, String>,
        depth: usize,
    ) -> OpKind {
        match &p.ty {
            TypeExpr::Named { name, .. } if name.is_empty() => {
                if let Some(concrete) = subst.get(&p.name) {
                    return self.kind_of_named(concrete, depth);
                }
                self.kind_of_named(&p.name, depth)
            }
            _ => self.kind_of_ty_with_subst(&p.ty, subst, depth),
        }
    }

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
            TypeExpr::Option { elem, .. } => OpKind::Option(Box::new(self.kind_of_ty(elem, depth + 1))),
            TypeExpr::Fn { params, results, .. } => OpKind::Fn(
                results
                    .iter()
                    .map(|r| self.kind_of_ty(r, depth + 1))
                    .collect(),
                Some(params.iter().map(|p| self.param_kind(p, depth + 1)).collect()),
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
            "float32" | "float64" | "f32" | "f64" | "complex64" | "complex128" | "c64" | "c128" => {
                OpKind::Float
            }
            "int" | "uint" | "i8" | "int8" | "i16" | "int16" | "i32" | "int32" | "i64" | "int64" | "u8"
            | "uint8" | "u16" | "uint16" | "u32" | "uint32" | "u64" | "uint64" | "usize"
            | "isize" | "uintptr" | "byte" | "rune" | "char" => OpKind::Int,
            // A curated foreign named type (`os.file`, `bufio.reader`,
            // `net.conn`, `serde_json.value`, ...) is resolved, not
            // unresolved: the same table #3521 uses at move boundaries. It
            // keeps its own kind so `kinds_conflict` can refuse scalar
            // interop; every non-scalar pairing stays fail-open there.
            // Handles are NOT exempt: `serde_json.value` lowers to the
            // `jsonbinding.Value` struct, so `var n int = v` dies in
            // `go build` exactly like `os.file` does.
            _ => match crate::foreign::foreign_named_type(name) {
                Some(
                    crate::model::TypeShape::ForeignOpaque(n)
                    | crate::model::TypeShape::OwnHandle(n)
                    | crate::model::TypeShape::BorrowHandle(n),
                ) => OpKind::Foreign(n),
                _ => OpKind::Unknown,
            },
        }
    }

    fn kind_of_type_str(&self, s: &str, depth: usize) -> OpKind {
        if depth > 8 {
            return OpKind::Unknown;
        }
        let s = s.trim();
        if let Some(fam) = CompFamily::split(s) {
            return match fam {
                CompFamily::Pointer { base } => {
                    OpKind::Pointer(Box::new(self.kind_of_type_str(base, depth + 1)))
                }
                CompFamily::Slice { elem } => {
                    OpKind::Slice(Box::new(self.kind_of_type_str(elem, depth + 1)))
                }
                CompFamily::Array { elem, .. } => {
                    OpKind::Array(Box::new(self.kind_of_type_str(elem, depth + 1)))
                }
                CompFamily::Map { key, value } => OpKind::Map(
                    Box::new(self.kind_of_type_str(key, depth + 1)),
                    Box::new(self.kind_of_type_str(value, depth + 1)),
                ),
                CompFamily::Chan { elem, .. } => {
                    OpKind::Chan(Box::new(self.kind_of_type_str(elem, depth + 1)))
                }
            };
        }
        if let Some(rest) = s.strip_prefix("[...]") {
            return OpKind::Array(Box::new(self.kind_of_type_str(rest, depth + 1)));
        }
        if let Some(rest) = s.strip_prefix("option[").and_then(|r| r.strip_suffix(']')) {
            return OpKind::Option(Box::new(self.kind_of_type_str(rest, depth + 1)));
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
            TypeExpr::Option { elem, .. } => {
                OpKind::Option(Box::new(self.kind_of_ty_with_subst(elem, subst, depth + 1)))
            }
            TypeExpr::Fn { params, results, .. } => OpKind::Fn(
                results
                    .iter()
                    .map(|r| self.kind_of_ty_with_subst(r, subst, depth + 1))
                    .collect(),
                Some(
                    params
                        .iter()
                        .map(|p| self.param_kind_with_subst(p, subst, depth + 1))
                        .collect(),
                ),
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

    fn struct_field_type_name(&self, k: &OpKind, field: &str) -> Option<String> {
        let full_name = self.struct_name(k)?;
        let base_name = Checker::base_named_type_str(&full_name);
        let fields = self.ck.structs.get(base_name)?;
        let subst = self.struct_type_arg_map(&full_name);
        let (_, ty) = fields.iter().find(|(n, _)| n == field)?;
        let rendered = ty.render();
        Some(subst.get(&rendered).cloned().unwrap_or(rendered))
    }

    /// #2858 — canonical numeric scalar of a struct field.
    fn struct_field_scalar(&self, k: &OpKind, field: &str) -> Option<&'static str> {
        let full_name = self.struct_name(k)?;
        let base_name = Checker::base_named_type_str(&full_name);
        let fields = self.ck.structs.get(base_name)?;
        let subst = self.struct_type_arg_map(&full_name);
        let (_, ty) = fields.iter().find(|(n, _)| n == field)?;
        if let TypeExpr::Named { name, .. } = ty {
            if let Some(arg) = subst.get(name.as_str()) {
                return self.scalar_of_type_name(arg);
            }
        }
        self.scalar_of_ty(ty)
    }

    /// #2858 — canonical numeric element/value scalar of a container struct field (e.g. `items []i64`).
    fn struct_field_container_elem_scalar(&self, k: &OpKind, field: &str) -> Option<&'static str> {
        let full_name = self.struct_name(k)?;
        let base_name = Checker::base_named_type_str(&full_name);
        let fields = self.ck.structs.get(base_name)?;
        let subst = self.struct_type_arg_map(&full_name);
        let (_, ty) = fields.iter().find(|(n, _)| n == field)?;
        let resolved = self.ck.resolve_type_alias(ty);
        match &resolved {
            TypeExpr::Slice { elem, .. } | TypeExpr::Array { elem, .. } => {
                if let TypeExpr::Named { name, .. } = elem.as_ref() {
                    if let Some(arg) = subst.get(name.as_str()) {
                        return self.scalar_of_type_name(arg);
                    }
                }
                self.scalar_of_ty(elem)
            }
            TypeExpr::Map { value, .. } => {
                if let TypeExpr::Named { name, .. } = value.as_ref() {
                    if let Some(arg) = subst.get(name.as_str()) {
                        return self.scalar_of_type_name(arg);
                    }
                }
                self.scalar_of_ty(value)
            }
            TypeExpr::Named { name, .. } => {
                if let Some(arg) = subst.get(name.as_str()) {
                    if let Some(fam) = CompFamily::split(arg.trim()) {
                        return match fam {
                            CompFamily::Slice { elem } | CompFamily::Array { elem, .. } => {
                                self.scalar_of_type_name(elem)
                            }
                            CompFamily::Map { value, .. } => self.scalar_of_type_name(value),
                            _ => None,
                        };
                    }
                }
                None
            }
            _ => None,
        }
    }

    pub(crate) fn kind_of_expr(&self, e: &Expr) -> OpKind {
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
                        Some(f.params.iter().map(|p| self.param_kind(p, 0)).collect()),
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
                if let Some(k) = self.struct_field_kind(&bk, name) {
                    return k;
                }
                // #682 U1: a method value (`f := z.add`) is a function value
                // whose receiver is already bound, so its arity is the
                // declaration's parameter count — exactly what `OpKind::Fn`'s
                // count slot carries for closures. Without this the binding
                // fell through to `Unknown` (fail-open) and `f(1, 2)` reached
                // emission, where Go refuses `too many arguments in call to f`.
                let head = Checker::base_named_type_str(name);
                if let Some(sn) = self.struct_base_name(&bk) {
                    if let Some(m) = self.ck.method_decls.get(&(sn, head.to_string())) {
                        if m.type_params.is_empty() {
                            return OpKind::Fn(
                                m.results
                                    .iter()
                                    .map(|r| self.kind_of_ty(&r.ty, 0))
                                    .collect(),
                                Some(m.params.iter().map(|p| self.param_kind(p, 0)).collect()),
                            );
                        }
                    }
                }
                // A method expression (`g := pt.get`) names the method through
                // its TYPE, so the receiver becomes the first parameter: the
                // function value takes `params + 1` arguments.
                if let Some(m) = self.method_expr_decl(base, name) {
                    let mut params = Vec::with_capacity(m.params.len() + 1);
                    params.push(bk);
                    for p in &m.params {
                        params.push(self.param_kind(p, 0));
                    }
                    return OpKind::Fn(
                        m.results
                            .iter()
                            .map(|r| self.kind_of_ty(&r.ty, 0))
                            .collect(),
                        Some(params),
                    );
                }
                OpKind::Unknown
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
                Some(params.iter().map(|p| self.param_kind(p, 0)).collect()),
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
                    "len" | "cap" | "copy" => return Some(vec![OpKind::Int]),
                    "real" | "imag" => return Some(vec![OpKind::Float]),
                    "append" => {
                        let first = args
                            .first()
                            .map(|a| self.kind_of_expr(&a.value))
                            .unwrap_or_else(|| OpKind::Slice(Box::new(OpKind::Unknown)));
                        return Some(vec![first]);
                    }
                    "make" => return Some(vec![self.make_kind(args)]),
                    "new" => return Some(vec![self.new_kind(args)]),
                    "delete" | "close" | "print" | "println" | "panic" | "clear" => {
                        return Some(Vec::new());
                    }
                    "recover" => return Some(vec![OpKind::Interface]),
                    "min" | "max" => {
                        let res_kind = args
                            .iter()
                            .map(|a| self.kind_of_expr(&a.value))
                            .find(|k| !matches!(k, OpKind::Unknown | OpKind::Void))
                            .unwrap_or(OpKind::Unknown);
                        return Some(vec![res_kind]);
                    }
                    // #2823: `some(v)` produces `option[kind(v)]`.
                    "some" if !self.ck.fns.contains_key("some") => {
                        let payload = args
                            .first()
                            .map(|a| self.kind_of_expr(&a.value))
                            .unwrap_or(OpKind::Unknown);
                        return Some(vec![OpKind::Option(Box::new(payload))]);
                    }
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
                if let Expr::Ident { name: type_name, .. } = unwrap_parens(base) {
                    let head = Checker::base_named_type_str(name);
                    let assoc_key = format!("{}.{}", type_name, head);
                    if let Some(f) = self.ck.fns.get(&assoc_key) {
                        return Some(
                            f.results
                                .iter()
                                .map(|r| self.kind_of_ty(&r.ty, 0))
                                .collect(),
                        );
                    }
                }
                // #2823: the option payload methods carry a real result kind —
                // `unwrap()` / `or(d)` yield the payload, `is_some()` /
                // `is_none()` yield `bool`. Member lookup on an option stays
                // fail-open otherwise (that is #1671's territory).
                if let OpKind::Option(elem) = &bk {
                    match name.as_str() {
                        "unwrap" if args.is_empty() => return Some(vec![(**elem).clone()]),
                        "or" if args.len() == 1 => return Some(vec![(**elem).clone()]),
                        "is_some" | "is_none" if args.is_empty() => {
                            return Some(vec![OpKind::Bool])
                        }
                        _ => {}
                    }
                }
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

    fn new_kind(&self, args: &[CallArg]) -> OpKind {
        let Some(first) = args.first() else {
            return OpKind::Pointer(Box::new(OpKind::Unknown));
        };
        let bare = unwrap_parens(&first.value);
        let inner = match bare {
            Expr::Ident { name, .. } => {
                if self.lookup_kind(name).is_some() || self.file_consts.contains_key(name) {
                    OpKind::Unknown
                } else {
                    let k = self.kind_of_named(name, 0);
                    if k != OpKind::Unknown {
                        k
                    } else {
                        let head = Checker::base_named_type_str(name);
                        if self.ck.structs.contains_key(name.as_str())
                            || self.ck.structs.contains_key(head)
                        {
                            OpKind::Struct(Some(head.to_string()))
                        } else {
                            OpKind::Unknown
                        }
                    }
                }
            }
            Expr::Index { base, index, .. } => {
                self.kind_of_named(&format!("{}[{}]", base.render(), index.render()), 0)
            }
            Expr::SliceLit {
                elem: Some(t),
                items,
                ..
            } if items.is_empty() => match bare.as_array_lit() {
                Some((_, ie, _)) => OpKind::Array(Box::new(self.kind_of_ty(ie, 0))),
                None => OpKind::Slice(Box::new(self.kind_of_ty(t, 0))),
            },
            Expr::MapLit {
                key,
                value,
                entries,
                ..
            } if entries.is_empty() => OpKind::Map(
                Box::new(self.kind_of_ty(key, 0)),
                Box::new(self.kind_of_ty(value, 0)),
            ),
            Expr::StructLit {
                ty: Some(t),
                fields,
                ..
            } if fields.is_empty() => self.kind_of_ty(t, 0),
            _ => OpKind::Unknown,
        };
        OpKind::Pointer(Box::new(inner))
    }


    fn check_filepath_package_call(&mut self, member: &str, args: &[CallArg], span: Span) {
        if matches!(
            member,
            "separator"
                | "list_separator"
                | "err_bad_pattern"
                | "skip_all"
                | "skip_dir"
                | "walk_func"
        ) {
            self.emit_e1002(
                span,
                format!("cannot call `filepath.{member}`: `filepath.{member}` is a constant or type, not a function (HYB-E1002)"),
                &format!("`filepath.{member}` is a package constant or type, not a callable function"),
            );
            return;
        }

        let expected_arity = match member {
            "abs" | "base" | "clean" | "dir" | "eval_symlinks" | "ext" | "from_slash"
            | "is_abs" | "is_local" | "localize" | "split" | "split_list" | "to_slash"
            | "volume_name" => Some(1),
            "has_prefix" | "match" | "rel" | "walk" | "walk_dir" => Some(2),
            _ => None,
        };

        if let Some(expected) = expected_arity {
            if args.iter().any(|a| a.spread) {
                self.emit_e1002(
                    span,
                    format!("cannot use `...` in call to non-variadic `filepath.{member}` (HYB-E1002)"),
                    &format!("`filepath.{member}` does not accept variadic spread arguments"),
                );
            } else if args.len() != expected {
                let noun = if expected == 1 { "argument" } else { "arguments" };
                self.emit_e1002(
                    span,
                    format!("`filepath.{member}` takes {expected} {noun}, got {} (HYB-E1002)", args.len()),
                    &format!("provide exactly {expected} {noun} to `filepath.{member}`"),
                );
            }
        }
    }

    fn check_path_package_call(&mut self, member: &str, args: &[CallArg], span: Span) {
        if member == "err_bad_pattern" {
            self.emit_e1002(
                span,
                "cannot call `path.err_bad_pattern`: `path.err_bad_pattern` is an error variable, not a function (HYB-E1002)".to_string(),
                "`path.err_bad_pattern` is a package variable, not a callable function",
            );
            return;
        }

        let expected_arity = match member {
            "base" | "clean" | "dir" | "ext" | "is_abs" | "split" => Some(1),
            "match" => Some(2),
            _ => None,
        };

        if let Some(expected) = expected_arity {
            if args.iter().any(|a| a.spread) {
                self.emit_e1002(
                    span,
                    format!("cannot use `...` in call to non-variadic `path.{member}` (HYB-E1002)"),
                    &format!("`path.{member}` does not accept variadic spread arguments"),
                );
            } else if args.len() != expected {
                let noun = if expected == 1 { "argument" } else { "arguments" };
                self.emit_e1002(
                    span,
                    format!("`path.{member}` takes {expected} {noun}, got {} (HYB-E1002)", args.len()),
                    &format!("provide exactly {expected} {noun} to `path.{member}`"),
                );
            }
        }
    }
    fn check_url_package_call(&mut self, member: &str, args: &[CallArg], span: Span) {
        if matches!(
            member,
            "url" | "values" | "userinfo" | "error" | "escape_error" | "invalid_host_error"
        ) {
            self.emit_e1002(
                span,
                format!("cannot call `url.{member}`: type `url.{member}` is not a function (HYB-E1002)"),
                &format!("`url.{member}` is a type name, not a callable function"),
            );
            return;
        }

        let expected_arity = match member {
            "parse" | "parse_request_uri" | "parse_query" | "query_escape" | "query_unescape"
            | "path_escape" | "path_unescape" | "user" => Some(1),
            "user_password" => Some(2),
            _ => None,
        };

        if let Some(expected) = expected_arity {
            if args.iter().any(|a| a.spread) {
                self.emit_e1002(
                    span,
                    format!("cannot use `...` in call to non-variadic `url.{member}` (HYB-E1002)"),
                    &format!("`url.{member}` does not accept variadic spread arguments"),
                );
            } else if args.len() != expected {
                let noun = if expected == 1 { "argument" } else { "arguments" };
                self.emit_e1002(
                    span,
                    format!("`url.{member}` takes {expected} {noun}, got {} (HYB-E1002)", args.len()),
                    &format!("provide exactly {expected} {noun} to `url.{member}`"),
                );
            }
        } else if member == "join_path" && args.is_empty() {
            self.emit_e1002(
                span,
                "`url.join_path` requires at least 1 argument `(base[, ...])`, got 0 (HYB-E1002)".to_string(),
                "provide at least 1 path argument to `url.join_path`",
            );
        }
    }

    // Full `strconv` call surface — every member the member table in
    // foreign.rs blesses, with the arity of the Go name it lowers to
    // (`to_go_name`: snake case -> CamelCase, e.g. `parse_int` -> ParseInt,
    // `quote_rune_to_ascii` -> QuoteRuneToASCII). Arities are the Go
    // std signatures on the pinned toolchain (go1.26.9). Before this, any
    // strconv call passed `hyb check` with any arity and non-callable
    // members (constants `int_size`, package error vars `err_range` /
    // `err_syntax`, struct type `num_error`) could be called as functions —
    // both blow up at `go build` instead (Row 13 BUILDABLE: reject
    // fail-closed, HYB-E1002). Never over-reject: only members the table
    // blesses are listed, and every accepted call matches its Go signature.
    fn check_strconv_package_call(&mut self, member: &str, args: &[CallArg], span: Span) {
        // Package-level values and types are not callable.
        if matches!(member, "int_size" | "err_range" | "err_syntax" | "num_error") {
            self.emit_e1002(
                span,
                format!("`strconv.{member}` is not a function — it is a package-level value/type and cannot be called (HYB-E1002)"),
                &format!("reference `strconv.{member}` directly, do not call it"),
            );
            return;
        }

        let expected_arity = match member {
            // 1 arg: Atoi(s), Itoa(i), ParseBool(str), FormatBool(b),
            // CanBackquote(s), IsGraphic(r), IsPrint(r), Quote(s),
            // QuoteRune(r), QuoteRuneToASCII(r), QuoteRuneToGraphic(r),
            // QuoteToASCII(s), QuoteToGraphic(s), QuotedPrefix(s),
            // Unquote(s)
            "atoi" | "itoa" | "parse_bool" | "format_bool" | "can_backquote"
            | "is_graphic" | "is_print" | "quote" | "quote_rune"
            | "quote_rune_to_ascii" | "quote_rune_to_graphic" | "quote_to_ascii"
            | "quote_to_graphic" | "quoted_prefix" | "unquote" => Some(1),
            // 2 args: ParseComplex(s, bitSize), ParseFloat(s, bitSize),
            // FormatInt(i, base), FormatUint(u, base), AppendBool(dst, b),
            // AppendQuote(dst, s), AppendQuoteRune(dst, r),
            // AppendQuoteRuneToASCII(dst, r), AppendQuoteRuneToGraphic(dst, r),
            // AppendQuoteToASCII(dst, s), AppendQuoteToGraphic(dst, s),
            // UnquoteChar(s, quote)
            "parse_complex" | "parse_float" | "format_int" | "format_uint"
            | "append_bool" | "append_quote" | "append_quote_rune"
            | "append_quote_rune_to_ascii" | "append_quote_rune_to_graphic"
            | "append_quote_to_ascii" | "append_quote_to_graphic"
            | "unquote_char" => Some(2),
            // 3 args: ParseInt(s, base, bitSize), ParseUint(s, base, bitSize),
            // AppendInt(dst, i, base), AppendUint(dst, u, base)
            "parse_int" | "parse_uint" | "append_int" | "append_uint" => Some(3),
            // 4 args: FormatComplex(c, fmt, prec, bitSize),
            // FormatFloat(f, fmt, prec, bitSize)
            "format_complex" | "format_float" => Some(4),
            // 5 args: AppendFloat(dst, f, fmt, prec, bitSize)
            "append_float" => Some(5),
            _ => None,
        };

        if let Some(expected) = expected_arity {
            if args.iter().any(|a| a.spread) {
                self.emit_e1002(
                    span,
                    format!("cannot use `...` in call to non-variadic `strconv.{member}` (HYB-E1002)"),
                    &format!("`strconv.{member}` does not accept variadic spread arguments"),
                );
            } else if args.len() != expected {
                let go_name = {
                    // mirror hy-emit's to_go_name without a cross-crate dep
                    // (hy-check must not depend on hy-emit).
                    let mut out = String::new();
                    for part in member.split('_') {
                        if part.is_empty() {
                            continue;
                        }
                        let mut cs = part.chars();
                        if let Some(c) = cs.next() {
                            out.extend(c.to_uppercase());
                        }
                        out.push_str(cs.as_str());
                    }
                    out
                };
                self.emit_e1002(
                    span,
                    format!(
                        "`strconv.{member}` takes exactly {expected} argument{}, got {} (HYB-E1002)",
                        if expected == 1 { "" } else { "s" },
                        args.len()
                    ),
                    &format!(
                        "match the arity of the Go function this lowers to (`strconv.{go_name}`)"
                    ),
                );
            }
        }
    }

    fn check_sort_package_call(&mut self, member: &str, args: &[CallArg], span: Span) {
        if matches!(member, "interface" | "int_slice" | "float64_slice" | "string_slice") {
            self.emit_e1002(
                span,
                format!("cannot call `sort.{member}`: `sort.{member}` is a type, not a function (HYB-E1002)"),
                &format!("`sort.{member}` is an interface or type name, not a callable function"),
            );
            return;
        }

        let expected_arity = match member {
            "ints" | "ints_are_sorted" | "float64s" | "float64s_are_sorted" | "strings"
            | "strings_are_sorted" | "sort" | "stable" | "is_sorted" | "reverse" => Some(1),
            "search_ints" | "search_float64s" | "search_strings" | "search" | "slice"
            | "slice_is_sorted" | "slice_stable" => Some(2),
            _ => None,
        };

        if let Some(expected) = expected_arity {
            if args.iter().any(|a| a.spread) {
                self.emit_e1002(
                    span,
                    format!("cannot use `...` in call to non-variadic `sort.{member}` (HYB-E1002)"),
                    &format!("`sort.{member}` does not accept variadic spread arguments"),
                );
            } else if args.len() != expected {
                let noun = if expected == 1 { "argument" } else { "arguments" };
                self.emit_e1002(
                    span,
                    format!("`sort.{member}` takes {expected} {noun}, got {} (HYB-E1002)", args.len()),
                    &format!("provide exactly {expected} {noun} to `sort.{member}`"),
                );
            }
        }
    }

    fn check_sync_package_call(&mut self, member: &str, args: &[CallArg], span: Span) {
        if matches!(
            member,
            "mutex"
                | "rw_mutex"
                | "rwmutex"
                | "wait_group"
                | "waitgroup"
                | "once"
                | "cond"
                | "map"
                | "pool"
                | "locker"
        ) {
            self.emit_e1002(
                span,
                format!("cannot call `sync.{member}`: `sync.{member}` is a type or interface, not a function (HYB-E1002)"),
                &format!("`sync.{member}` is a package type or interface, not a callable function"),
            );
            return;
        }

        if args.iter().any(|a| a.spread) {
            self.emit_e1002(
                span,
                format!("cannot use `...` in call to non-variadic `sync.{member}` (HYB-E1002)"),
                &format!("`sync.{member}` does not accept variadic spread arguments"),
            );
            return;
        }

        let expected_arity = match member {
            "new_cond" | "once_func" | "once_value" | "once_values" => Some(1),
            _ => None,
        };

        if let Some(expected) = expected_arity {
            if args.len() != expected {
                let noun = if expected == 1 { "argument" } else { "arguments" };
                self.emit_e1002(
                    span,
                    format!("`sync.{member}` takes {expected} {noun}, got {} (HYB-E1002)", args.len()),
                    &format!("provide exactly {expected} {noun} to `sync.{member}`"),
                );
            }
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
    /// Value-position type names in composite literals (`b := Undeclared{}`,
    /// `xs := []Undeclared{...}`, `m := map[K]Undeclared{...}`). The literal's
    /// own `ty` routes through `self.ty()` — the `Expr::` side of the
    /// checker — which resolves arity/instantiation but NOT whether the
    /// named type exists. A name nothing declares passes `hyb check` with 0
    /// diagnostics, the emitter prints the literal verbatim, and `go build`
    /// fails with `undefined: <name>` — Row 13 BUILDABLE. Fail closed with
    /// the existing BUILDABLE code. Deliberately fail-open for Go predeclared
    /// / conversion-target names (`int{}`-style scalar literals) and dotted
    /// foreign/shapes types: those live on a different root (scalar/interface
    /// composite literals) and their members are emitted verbatim by design.
    fn check_value_position_type_name(&mut self, ty: &TypeExpr) {
        let TypeExpr::Named { name, span, .. } = ty else { return };
        let name = name.trim();
        if name.is_empty() || name.contains('.') || name != Checker::base_named_type_str(name) {
            return;
        }
        if crate::const_init::is_conversion_type_name(name)
            || crate::resolve::is_predeclared_type_name(name)
        {
            return;
        }
        // Prelude types (both a literal-typed element and the prelude carriers)
        if name == "option" || name == "set" {
            return;
        }
        if self.ck.structs.contains_key(name)
            || self.ck.enums.contains_key(name)
            || self.ck.type_aliases.contains_key(name)
            // Zero-field structs (`struct svc {}`) are recorded as
            // package-scope names, not in `structs` — same carve-out the
            // declaration-site checker uses.
            || self.ck.pkg_names.get(name) == Some(&"struct")
            // Type parameters of the enclosing generic signature are legal
            // composite-literal spellings inside it (`fn new[k, v any]` →
            // `map[k]v{}`; Go generic signatures scope those names locally).
            || self.fn_type_params.contains(name)
        {
            return;
        }
        self.emit_e1002(
            *span,
            format!(
                "type `{}` is not declared — Go refuses `undefined: {}` in the emitted composite literal (HYB-E1002)",
                name, name
            ),
            "value-position type names must resolve: declare the struct/enum/alias, or use its declared spelling",
        );
    }

    fn check_struct_literal(
        &mut self,
        ty: Option<&TypeExpr>,
        fields: &[FieldInit],
        span: Span,
    ) {
        for f in fields {
            self.check_single_value_position(&f.value, "a struct literal field", false);
        }
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
                    if self.non_single_value_count(&f.value).is_some() {
                        continue;
                    }
                    let want = self.kind_of_ty_with_subst(fty, &subst, 0);
                    let got = self.kind_of_expr(&f.value);
                    let is_untyped_int_to_float = want == OpKind::Float
                        && got == OpKind::Int
                        && self.is_untyped_int_const(&f.value);
                    // #2782: `want`/`got` both collapse to `Int`/`Float`, but
                    // the field's own Go spelling may differ from the typed
                    // initializer's (`int` into `int64`, `float64` into
                    // `float32`) — Go refuses that in a struct literal.
                    let scalar_mismatch =
                        self.scalar_names_conflict(self.scalar_of_ty(fty), self.typed_value_scalar(&f.value));
                    let rendered_fty = fty.render();
                    let substituted_fty = if subst.is_empty() {
                        rendered_fty
                    } else {
                        Self::substitute_type_str(&rendered_fty, &subst)
                    };
                    let field_type_name = substituted_fty.as_str();
                    let got_type_name = self.typed_value_type_name(&f.value);
                    let is_generic_ty = |s: &str| {
                        let inner = s.trim_start_matches("[]").trim_start_matches('*');
                        inner.len() == 1 && inner.chars().all(|c| c.is_ascii_lowercase())
                    };
                    let container_or_alias_mismatch = match (&got_type_name, &want, &got) {
                        (Some(gname), OpKind::Chan(_), OpKind::Chan(_))
                        | (Some(gname), OpKind::Map(_, _), OpKind::Map(_, _))
                        | (Some(gname), OpKind::Slice(_), OpKind::Slice(_))
                        | (Some(gname), OpKind::Pointer(_), OpKind::Pointer(_))
                        | (Some(gname), OpKind::Array(_), OpKind::Array(_)) => {
                            if field_type_name == gname.as_str() || is_generic_ty(field_type_name) || is_generic_ty(gname) {
                                false
                            } else {
                                true
                            }
                        }
                        (Some(gname), _, _) => self.sized_array_names_differ(Some(field_type_name), Some(gname.as_str())),
                        _ => false,
                    };
                    let array_mismatch = self.sized_array_names_differ(
                        Some(field_type_name),
                        got_type_name.as_deref(),
                    );
                    let conflict = (want == OpKind::Float && got == OpKind::Int && !is_untyped_int_to_float)
                        || kinds_conflict(&want, &got)
                        || scalar_mismatch
                        || container_or_alias_mismatch
                        || array_mismatch;
                    let already_diagnosed_e3012 = self
                        .ck
                        .diags
                        .iter()
                        .chain(self.diags.iter())
                        .any(|d| d.code == "HYB-E3012" && d.span == f.value.span());
                    if conflict && !already_diagnosed_e3012 {
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
    /// #2811 — the element spelling of a slice TYPE NAME (alias resolution
    /// included), e.g. `my_int64_slice` → `int64`. `None` when the name does
    /// not resolve to a slice type, which fails open.
    fn named_slice_element_name(&self, name: &str) -> Option<String> {
        let base = Checker::base_named_type_str(name);
        let ty = self.ck.type_aliases.get(base)?;
        match self.ck.resolve_type_alias(ty) {
            hy_syntax::TypeExpr::Slice { elem, .. } => Some(elem.render()),
            _ => None,
        }
    }

    /// #2811 — a `string`↔slice conversion whose element is a KNOWN numeric
    /// type other than `byte`/`rune`. Go's conversion rules admit only
    /// `[]byte`/`[]uint8` and `[]rune`/`[]int32`; `OpKind::Int` collapses every
    /// integer width, so `string([]int64)` and `my_int64_slice(s)` passed the
    /// kind check and failed `go build` (`cannot convert ... to type string`).
    ///
    /// Only a KNOWN numeric spelling that is not `uint8`/`int32` counts: a
    /// user type, an alias of an alias or an unresolved spelling fails open,
    /// and the non-`Int` element kinds (`[]string`, `[]bool`, `[]float64`) are
    /// already rejected by `conversion_kinds_conflict`.
    fn string_slice_conversion_element_mismatch(
        &self,
        name: &str,
        target: &OpKind,
        arg: &Expr,
        arg_kind: &OpKind,
    ) -> bool {
        let elem = if matches!(target, OpKind::Str) {
            // `string(xs)` — the OPERAND carries the slice spelling.
            if !matches!(arg_kind, OpKind::Slice(_) | OpKind::Array(_)) {
                return false;
            }
            self.slice_element_type_name(arg)
        } else if matches!(arg_kind, OpKind::Str) && matches!(target, OpKind::Slice(_)) {
            // `T(s)` where `T` names a slice type.
            self.named_slice_element_name(name)
        } else {
            return false;
        };
        matches!(
            elem.as_deref().and_then(canonical_numeric_name),
            Some(x) if x != "uint8" && x != "int32"
        )
    }

    fn find_untyped_nonconst_shift<'b>(&self, e: &'b Expr) -> Option<(&'b Expr, BinOp)> {
        match unwrap_parens(e) {
            Expr::Binary {
                op: op @ (BinOp::Shl | BinOp::Shr),
                lhs,
                rhs,
                ..
            } => {
                if self.is_untyped_int_const(lhs) && !self.is_untyped_int_const(rhs) {
                    Some((e, *op))
                } else {
                    self.find_untyped_nonconst_shift(lhs)
                        .or_else(|| self.find_untyped_nonconst_shift(rhs))
                }
            }
            Expr::Binary { lhs, rhs, .. } => {
                self.find_untyped_nonconst_shift(lhs)
                    .or_else(|| self.find_untyped_nonconst_shift(rhs))
            }
            Expr::Unary { operand, .. } => self.find_untyped_nonconst_shift(operand),
            _ => None,
        }
    }

    fn check_conversion_arity(&mut self, name: &str, args: &[CallArg], span: Span) {
        let k = self.kind_of_named(name, 0);
        if matches!(k, OpKind::Unknown) {
            return;
        }
        if args.len() != 1 {
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
            return;
        }
        let arg = &args[0].value;
        self.check_single_value_position(arg, "a type conversion", /*binding_position=*/ false);

        // #3767: In Go specification (§Operators / Arithmetic operators / Integer operators):
        // "If the left operand of a non-constant shift expression is an untyped constant,
        // it is first implicitly converted to the type it would assume if the shift expression
        // were replaced by its left operand alone."
        // When an untyped shift (`1 << n`) appears inside a float/complex conversion
        // (`float64(...)`, `complex128(...)`), the untyped constant left operand adopts
        // the non-integer target type, which Go forbids with `invalid operation: 1 << n (shift of type float64)`.
        if matches!(
            name,
            "float32" | "float64" | "f32" | "f64" | "complex64" | "complex128" | "c64" | "c128"
        ) {
            if let Some((shift_expr, _)) = self.find_untyped_nonconst_shift(arg) {
                self.emit_e1002(
                    shift_expr.span(),
                    format!(
                        "invalid operation `{}`: shift of type `{}` in conversion to `{}` (HYB-E1002)",
                        shift_expr.render(),
                        name,
                        name
                    ),
                    "a non-constant shift of an untyped constant inherits the target type of the enclosing conversion; bitwise shifts are not defined on floating-point or complex types",
                );
                return;
            }
        }

        let arg_k = self.kind_of_expr(arg);
        // #2811: `string`↔slice conversions are restricted to byte/rune
        // elements; `OpKind::Int` cannot tell `int64` from `uint8`.
        let element_mismatch = self.string_slice_conversion_element_mismatch(name, &k, arg, &arg_k);
        if conversion_kinds_conflict(&k, &arg_k) || element_mismatch {
            let note = if matches!(k, OpKind::Struct(_)) {
                "structs cannot be instantiated via function-call conversions; write a struct literal like `T{...}` instead"
            } else {
                "in Go, type conversions `T(x)` require compatible types"
            };
            self.emit_e1002(
                arg.span(),
                format!(
                    "cannot convert `{}` (type `{}`) to type `{}` (HYB-E1002)",
                    arg.render(),
                    arg_k.label(),
                    name
                ),
                note,
            );
        }
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
            // `f()?` unwraps the trailing `error` result. A void fallible
            // function therefore yields no value and a multi-value one still
            // yields its remaining N-1 values; both are illegal in a
            // single-value position. A `(T, error)` call unwraps to exactly
            // one value and stays legal.
            Expr::Try { operand, .. } => {
                let Expr::Call { callee, args, .. } = unwrap_parens(operand) else {
                    return None;
                };
                // `f()?` unwraps a trailing `error`. Only a local fn or a
                // closure has a declared result list; a boundary or handle
                // method (`doc.array("users")?`) is fail-open.
                let results: &[FnResult] = match unwrap_parens(callee) {
                    Expr::Ident { name, .. } => match self.ck.fns.get(name.as_str()) {
                        Some(f) => &f.results,
                        None => return None,
                    },
                    Expr::FnLit { results, .. } => results,
                    _ => return None,
                };
                // A `?` on a non-error result is an accepted Hybrid no-op; do
                // not reinterpret it as a void unwrap.
                if !matches!(
                    results.last().map(|r| &r.ty),
                    Some(TypeExpr::Named { name, .. }) if name == "error"
                ) {
                    return None;
                }
                let res = self.call_result_kinds(callee, args)?;
                let effective = res.len().saturating_sub(1);
                if effective == 1 {
                    None
                } else {
                    Some(effective)
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

    /// SPEC-LANGUAGE §4 G5 — call-site type inference follows Go's rules:
    /// a type argument is inferred only from the argument types, so a type
    /// parameter that appears in no parameter type (`fn mk[t any]() t`,
    /// `fn conv[a any, b any](x a) b`) cannot be inferred from a bare call
    /// `mk()` / `conv(1)`, whatever the result is assigned to. Go refuses the
    /// emitted call with `in call to mk, cannot infer t`; the explicit form
    /// `mk[int]()` (an `Expr::Index` callee) never reaches this check.
    ///
    /// Only the MVP constraints (`any`, `comparable`; G3) are handled: with
    /// a core-type constraint Go could infer a parameter from another
    /// parameter's constraint, so any other constraint skips the check.
    fn check_uninferable_type_params(&mut self, name: &str, f: &FnDecl, span: Span) {
        // An explicit instantiation reaches here folded to `mk[int]`.
        if f.receiver.is_some() || name.contains('[') {
            return;
        }
        if f.type_params.iter().any(|tp| {
            !matches!(
                tp.constraint.as_deref(),
                None | Some("any") | Some("comparable")
            )
        }) {
            return;
        }
        let mut mentioned: HashSet<String> = HashSet::new();
        // `TypeExpr::render` folds fn types to `fn`, so walk the tree; a
        // named type keeps its type arguments in the name (`box[t]`).
        fn collect(ty: &TypeExpr, out: &mut HashSet<String>) {
            match ty {
                TypeExpr::Named { name, .. } => {
                    for tok in name.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
                        if !tok.is_empty() {
                            out.insert(tok.to_string());
                        }
                    }
                }
                TypeExpr::Pointer { to: e, .. }
                | TypeExpr::Slice { elem: e, .. }
                | TypeExpr::Array { elem: e, .. }
                | TypeExpr::Chan { elem: e, .. }
                | TypeExpr::Option { elem: e, .. } => collect(e, out),
                TypeExpr::Map { key, value, .. } => {
                    collect(key, out);
                    collect(value, out);
                }
                TypeExpr::Fn {
                    params, results, ..
                } => {
                    for p in params {
                        collect(&p.ty, out);
                    }
                    for r in results {
                        collect(r, out);
                    }
                }
            }
        }
        for p in &f.params {
            collect(&p.ty, &mut mentioned);
        }
        let missing: Vec<&str> = f
            .type_params
            .iter()
            .map(|tp| tp.name.as_str())
            .filter(|n| !mentioned.contains(*n))
            .collect();
        let Some(first) = missing.first() else {
            return;
        };
        let list = missing
            .iter()
            .map(|n| format!("`{}`", n))
            .collect::<Vec<_>>()
            .join(", ");
        self.emit_e1002(
            span,
            format!(
                "cannot infer type parameter {} of `{}`: it appears in no parameter type — Go refuses `in call to {}, cannot infer {}` (HYB-E1002)",
                list, name, name, first
            ),
            "instantiate explicitly (`f[int](...)`): Go infers type arguments only from argument types (SPEC-LANGUAGE §4 G5), never from the result or the assignment target",
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
    /// #1961 — generic calls failed open end to end: `check_user_fn_arity`
    /// bailed on `!f.type_params.is_empty()` and `check_user_fn_arg_types`
    /// did the same, so `id[int](10, 20, 30)` (explicit instantiation) and
    /// `id(10, 20, 30)` (inferred) passed `hyb check` with zero diagnostics
    /// and failed `go build`. The type arguments themselves stay
    /// uninferrable; this check owns only the argument count and the
    /// explicit-instantiation type-argument count (Go refuses a wrong one
    /// with `got N type arguments but want M`).
    fn check_generic_call_arity(
        &mut self,
        name: &str,
        f: &FnDecl,
        args: &[CallArg],
        span: Span,
    ) {
        if name.contains('[') && name.ends_with(']') {
            // Explicit instantiation `id[int, string](...)`: Go refuses a
            // wrong type-argument count before it looks at the arguments.
            let open = name.find('[').unwrap();
            let got = split_top_level_type_args(&name[open + 1..name.len() - 1]).len();
            let want = f.type_params.len();
            if got != want {
                self.emit_e1002(
                    span,
                    format!(
                        "got {} type argument{} but want {} for `{}` — Go refuses `got {} type arguments but want {}` (HYB-E1002)",
                        got,
                        if got == 1 { "" } else { "s" },
                        want,
                        name,
                        got,
                        want
                    ),
                    "supply one type argument per declared type parameter",
                );
                return;
            }
        }
        // A lone multi-value call IS the whole argument list and expands to
        // one argument per result — #682 U7 owns that comparison (see
        // `check_user_fn_arity`), so leave it alone here.
        if self.multi_value_arg_count(args).is_some() {
            return;
        }
        let (want, got) = (f.params.len(), args.len());
        if want == got {
            return;
        }
        let go = if got < want {
            "not enough arguments in call to"
        } else {
            "too many arguments in call to"
        };
        self.emit_e1002(
            span,
            format!(
                "`{}` takes {} argument{} but is called with {} — Go refuses `{} {}` (HYB-E1002)",
                name,
                want,
                if want == 1 { "" } else { "s" },
                got,
                go,
                name
            ),
            "a call must pass one argument per declared parameter; the emitted Go call is rejected at build time",
        );
    }

    fn check_user_fn_arity(&mut self, name: &str, args: &[CallArg], span: Span) {
        let head = Checker::base_named_type_str(name);
        let Some(f) = self.ck.fns.get(name).or_else(|| self.ck.fns.get(head)) else {
            return;
        };
        // #2701: Hybrid declares no variadic parameters (`parse_params` never
        // accepts `...T`), so a spread argument on a user function is never
        // buildable — Go refuses `cannot use ... in call to non-variadic <f>`.
        // The `...` spread form is only legal on the variadic built-in `append`.
        if args.iter().any(|a| a.spread) {
            self.emit_e1002(
                span,
                format!(
                    "cannot use `...` in call to non-variadic `{}` — Go refuses `cannot use ... in call to non-variadic {}` (HYB-E1002)",
                    name, name
                ),
                "`...` (variadic spread) is only allowed on the variadic built-in `append`; Hybrid declares no variadic parameters",
            );
            return;
        }
        // Generic instantiations (`id[int](x)`) carry type arguments the
        // parameter list does not describe; leave those to the type
        // checker. #1961: the argument *count* is still checkable — the
        // emitted Go call fails `go build` verbatim today
        // (`too many arguments in call to id[int]` / `... to id`) — so a
        // wrong-count explicit instantiation or inferred generic call is
        // diagnosed here instead of failing open.
        if !f.type_params.is_empty() {
            self.check_generic_call_arity(name, f, args, span);
            self.check_uninferable_type_params(name, f, span);
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
    /// `pt.get` where `pt` is a type name (not a local) that declares a
    /// non-generic VALUE-receiver method `get`: a Go method expression whose
    /// function value takes the receiver as its first argument. Pointer
    /// receivers (`(*pt).get`) and generic methods fail open.
    fn method_expr_decl(&self, base: &Expr, name: &str) -> Option<&'a FnDecl> {
        let Expr::Ident { name: tn, .. } = unwrap_parens(base) else {
            return None;
        };
        if self.lookup_kind(tn).is_some() || self.scopes.iter().any(|s| s.contains_key(tn)) {
            return None;
        }
        let m: &'a FnDecl = *self.ck.method_decls.get(&(tn.clone(), name.to_string()))?;
        if !m.type_params.is_empty() || m.receiver.as_ref().is_some_and(|r| r.is_pointer) {
            return None;
        }
        Some(m)
    }

    /// #2834: a method-expression call `pt.get(recv, args...)` was
    /// never checked (the type-name base has no kind, so every method/field
    /// check failed open): `pt.get(1)` and `pt.get()` passed `hyb check` and
    /// failed `go build` (`cannot use 1 ... as pt value in argument to
    /// pt.get` / `not enough arguments in call to pt.get`). The receiver is
    /// argument 0, the declared parameters follow.
    fn check_method_expr_call(
        &mut self,
        base: &Expr,
        name: &str,
        m: &'a FnDecl,
        args: &[CallArg],
        span: Span,
    ) {
        let tn = base.render();
        if args.iter().any(|a| a.spread) || self.multi_value_arg_count(args).is_some() {
            return;
        }
        let want = m.params.len() + 1;
        let got = args.len();
        if got != want {
            self.emit_e1002(
                span,
                format!(
                    "method expression `{}.{}` takes {} argument{} (the receiver, then the method's parameters) but is called with {} — Go refuses `{} arguments in call to {}.{}` (HYB-E1002)",
                    tn,
                    name,
                    want,
                    if want == 1 { "" } else { "s" },
                    got,
                    if got < want { "not enough" } else { "too many" },
                    tn,
                    name
                ),
                "a method expression `T.m` is a function whose first argument is the receiver: call it as `T.m(recv, args...)`, or call the method on a value: `recv.m(args...)`",
            );
            return;
        }
        let recv_kind = OpKind::Struct(Some(tn.clone()));
        let recv = &args[0].value;
        let got_kind = self.kind_of_expr(recv);
        if kinds_conflict(&recv_kind, &got_kind) {
            self.emit_e1002(
                recv.span(),
                format!(
                    "cannot use `{}` (value of type `{}`) as `{}` value in argument to `{}.{}` — the first argument of a method expression is the receiver (HYB-E1002)",
                    recv.render(),
                    got_kind.label(),
                    tn,
                    tn,
                    name
                ),
                "pass a receiver value of the method's type as the first argument",
            );
        }
        self.check_method_arg_types(&recv_kind, name, &args[1..]);
    }

    fn check_method_arity(&mut self, bk: &OpKind, name: &str, args: &[CallArg], span: Span) {
        // #880: a generic receiver carries its type arguments (`cache[int]`,
        // `*lru_cache[k, v]`) while `method_decls` is keyed by the *base*
        // name, so the bare `struct_name` lookup missed every instantiated
        // receiver, early-returned, and exempted all of their method calls
        // from C9 — the checker accepted calls the emitted Go cannot build
        // (`not enough arguments in call to c.set`). Fold through
        // `struct_base_name`, exactly like the `has_method` and C10
        // (`call_result_kinds`) lookups a few lines above; `sn` is then the
        // base name, which is also what the diagnostic renders for plain
        // structs.
        let Some(sn) = self.struct_base_name(bk) else {
            return;
        };
        let Some(m) = self.ck.method_decls.get(&(sn.clone(), name.to_string())) else {
            return;
        };
        // #2701: a method is never variadic in Hybrid, so a spread argument is
        // never buildable — Go refuses `cannot use ... in call to non-variadic`.
        if args.iter().any(|a| a.spread) {
            self.emit_e1002(
                span,
                format!(
                    "cannot use `...` in call to non-variadic method `{}.{}` — Go refuses `cannot use ... in call to non-variadic {}.{}` (HYB-E1002)",
                    sn, name, sn, name
                ),
                "`...` (variadic spread) is only allowed on the variadic built-in `append`; methods are never variadic",
            );
            return;
        }
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
        // #1961: the argument count of an inferred generic call (`id(10,
        // 20)`) is now checked by `check_generic_call_arity` via
        // `check_user_fn_arity`; the type arguments themselves stay
        // uninferrable, so the per-argument type walk below keeps failing
        // open.
        if !f.type_params.is_empty() {
            return;
        }
        if let Some(n) = self.multi_value_arg_count(args) {
            if f.params.len() == n {
                let got_kinds = self.multi_value_kinds(&args[0].value);
                // #3293: `kinds_conflict` alone cannot see int32 vs int64 or
                // float32 vs float64 -- `OpKind` collapses both. Ask the
                // callee's own result scalars for the exact widths; a
                // non-named callee or an untyped result gives None and the
                // check fails open.
                let got_scalars = match unwrap_parens(&args[0].value) {
                    Expr::Call { callee, args, .. } => self.call_result_scalars(callee, args),
                    _ => None,
                };
                for (i, (p, got)) in f.params.iter().zip(got_kinds.iter()).enumerate() {
                    let want = self.kind_of_ty(&p.ty, 0);
                    let want_scalar = self.scalar_of_ty(&p.ty);
                    let got_scalar = got_scalars
                        .as_ref()
                        .and_then(|v| v.get(i).copied().flatten());
                    let width_conflict = matches!(
                        (want_scalar, got_scalar),
                        (Some(w), Some(g)) if w != g
                    );
                    if kinds_conflict(&want, got) || width_conflict {
                        let got_label = got_scalar
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| got.label());
                        let want_label = want_scalar
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| want.label());
                        self.emit_e1002(
                            args[0].value.span(),
                            format!(
                                "cannot use `{}` result of type `{}` as `{}` value in argument to `{}` — Go refuses mismatched argument types (HYB-E1002)",
                                args[0].value.render(),
                                got_label,
                                want_label,
                                name
                            ),
                            "argument types must match declared parameter types",
                        );
                    }
                }
            }
            return;
        }
        if f.params.len() != args.len() {
            return;
        }
        for (p, a) in f.params.iter().zip(args.iter()) {
            let want = self.kind_of_ty(&p.ty, 0);
            if Checker::is_nil_literal(&a.value) && is_non_nilable(&want) {
                self.emit_e1002(
                    a.value.span(),
                    format!(
                        "cannot use nil as `{}` value in argument to `{}` — Go refuses mismatched argument types (HYB-E1002)",
                        p.ty.render(),
                        name
                    ),
                    "argument types must match declared parameter types",
                );
                continue;
            }
            let got = self.kind_of_expr(&a.value);
            if let OpKind::Pointer(inner) = &want {
                if !kinds_conflict(inner, &got) && self.is_addressable_value_receiver(&a.value) {
                    continue;
                }
            }
            let int_to_float_conflict = want == OpKind::Float
                && got == OpKind::Int
                && !self.is_untyped_int_const(&a.value);
            // #2770: both sides share `OpKind::Float`/`Int` but differ in the
            // concrete type (`float32` vs `float64`, `int` vs `int64`).
            let scalar_mismatch = matches!(
                (self.scalar_of_ty(&p.ty), self.typed_value_scalar(&a.value)),
                (Some(w), Some(g)) if w != g
            );
            if kinds_conflict(&want, &got)
                || int_to_float_conflict
                || scalar_mismatch
                || self.sized_array_names_differ(
                    Some(p.ty.render().as_str()),
                    self.typed_value_type_name(&a.value).as_deref(),
                )
            {
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
            if Checker::is_nil_literal(&a.value) && is_non_nilable(&want) {
                self.emit_e1002(
                    a.value.span(),
                    format!(
                        "cannot use nil as `{}` value in argument to `{}.{}` — Go refuses mismatched argument types (HYB-E1002)",
                        p.ty.render(),
                        sn,
                        name
                    ),
                    "argument types must match declared parameter types",
                );
                continue;
            }
            let got = self.kind_of_expr(&a.value);
            if let OpKind::Pointer(inner) = &want {
                if !kinds_conflict(inner, &got) && self.is_addressable_value_receiver(&a.value) {
                    continue;
                }
            }
            let int_to_float_conflict = want == OpKind::Float
                && got == OpKind::Int
                && !self.is_untyped_int_const(&a.value);
            // #2770: concrete numeric type differing under one `OpKind`.
            let scalar_mismatch = matches!(
                (self.scalar_of_ty(&p.ty), self.typed_value_scalar(&a.value)),
                (Some(w), Some(g)) if w != g
            );
            if kinds_conflict(&want, &got)
                || int_to_float_conflict
                || scalar_mismatch
                || self.sized_array_names_differ(
                    Some(p.ty.render().as_str()),
                    self.typed_value_type_name(&a.value).as_deref(),
                )
            {
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

    fn check_fn_lit_call(&mut self, params: &[Param], args: &[CallArg], callee: &Expr, span: Span) {
        if self.multi_value_arg_count(args).is_some() {
            return;
        }
        let want = params.len();
        let got = args.len();
        if want != got {
            self.report_local_arity(&callee.render(), want, got, span);
            return;
        }
        for (p, a) in params.iter().zip(args.iter()) {
            let want = self.kind_of_ty(&p.ty, 0);
            if Checker::is_nil_literal(&a.value) && is_non_nilable(&want) {
                self.emit_e1002(
                    a.value.span(),
                    format!(
                        "cannot use nil as `{}` value in argument to func literal (HYB-E1002)",
                        p.ty.render()
                    ),
                    "argument types must match closure parameter types",
                );
                continue;
            }
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
                        "cannot use `{}` (value of type `{}`) as `{}` value in argument to func literal (HYB-E1002)",
                        a.value.render(),
                        got.label(),
                        want.label()
                    ),
                    "argument types must match closure parameter types",
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
                    let res_scalars = self.call_result_scalars(callee, args);
                    if res.len() == want.len() {
                        for (i, (r, w)) in res.iter().zip(want.iter()).enumerate() {
                            if !self.report_return_conflict(&values[0], r, w, values[0].span()) {
                                let got_s = res_scalars.as_ref().and_then(|rs| rs.get(i).copied().flatten());
                                self.report_numeric_return_mismatch(got_s, r, i, values[0].span());
                            }
                        }
                        return;
                    }
                    if !res.is_empty()
                        && self.fn_results_error_tail
                        && res.len() + 1 == want.len()
                    {
                        for (i, (r, w)) in res.iter().zip(want.iter()).enumerate() {
                            if !self.report_return_conflict(&values[0], r, w, values[0].span()) {
                                let got_s = res_scalars.as_ref().and_then(|rs| rs.get(i).copied().flatten());
                                self.report_numeric_return_mismatch(got_s, r, i, values[0].span());
                            }
                        }
                        return;
                    }
                    let (nr, nw) = (res.len(), want.len());
                    let msg = match (nw, nr) {
                        (0, _) => format!(
                            "`return` supplies `{}` with {nr} result{} in a function that declares no results — Go refuses `too many return values` (HYB-E1002)",
                            callee.render(),
                            if nr == 1 { "" } else { "s" }
                        ),
                        (_, 0) => format!(
                            "`return` supplies `{}` which returns no value (void) but the signature declares {nw} result{} — Go refuses `{} (no value) used as value in return statement` (HYB-E1002)",
                            callee.render(),
                            if nw == 1 { "" } else { "s" },
                            callee.render()
                        ),
                        _ => format!(
                            "`return` supplies `{}` with {nr} result{} but the signature declares {nw} result{} — Go refuses `{} return values` (HYB-E1002)",
                            callee.render(),
                            if nr == 1 { "" } else { "s" },
                            if nw == 1 { "" } else { "s" },
                            if nr < nw { "not enough" } else { "too many" }
                        ),
                    };
                    self.emit_e1002(
                        values[0].span(),
                        msg,
                        "a return statement must supply exactly the results the signature declares",
                    );
                    return;
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
                for (i, (v, w)) in values.iter().zip(want.iter()).enumerate() {
                    self.check_single_value_position(v, "a return statement", false);
                    let got = self.kind_of_expr(v);
                    if !self.report_return_conflict(v, &got, w, v.span()) {
                        let got_s = self.typed_value_scalar(v);
                        self.report_numeric_return_mismatch(got_s, &got, i, v.span());
                        self.report_array_return_mismatch(v, i, v.span());
                    }
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
        for (i, (v, w)) in values.iter().zip(want.iter()).enumerate() {
            self.check_single_value_position(v, "a return statement", false);
            let got = self.kind_of_expr(v);
            if !self.report_return_conflict(v, &got, w, v.span()) {
                let got_s = self.typed_value_scalar(v);
                self.report_numeric_return_mismatch(got_s, &got, i, v.span());
                self.report_array_return_mismatch(v, i, v.span());
            }
        }
    }

    fn report_array_return_mismatch(&mut self, val: &Expr, idx: usize, span: Span) {
        let Some(want_name) = self.fn_result_type_names.get(idx) else { return };
        let got_name = self.typed_value_type_name(val);
        if self.sized_array_names_differ(Some(want_name.as_str()), got_name.as_deref()) {
            let got_label = got_name.as_deref().unwrap_or("array");
            self.emit_e1002(
                span,
                format!(
                    "cannot return a value of type `{got_label}` from a function declaring `{want_name}` — Go refuses `cannot use ... as {want_name} value in return statement` (HYB-E1002)"
                ),
                "return values must match the declared result types exactly; array lengths cannot differ",
            );
        }
    }

    /// #1031 — a TYPED numeric value returned into a result of a different
    /// numeric type (`int` → `float64`, `int` → `int64`, `float32` →
    /// `float64`, ...). `kinds_conflict` cannot see these: `OpKind` collapses
    /// the widths, and `(Float, Int)` must stay legal for untyped constants.
    /// Go has no implicit numeric conversion for typed values, so the emitted
    /// `return` fails `go build`. Fires only when both exact types are known
    /// and the kind check has not already reported this value.
    fn report_numeric_return_mismatch(
        &mut self,
        got: Option<&'static str>,
        got_kind: &OpKind,
        idx: usize,
        span: Span,
    ) {
        let Some(got) = got else { return };
        let Some(want) = self.fn_result_scalars.get(idx).copied().flatten() else {
            return;
        };
        if got == want {
            return;
        }
        if let Some(want_kind) = self.fn_results.get(idx) {
            if kinds_conflict(want_kind, got_kind) {
                return;
            }
        }
        self.emit_e1002(
            span,
            format!(
                "cannot return a value of type `{got}` from a function declaring `{want}` — Go refuses `cannot use ... (variable of type {got}) as {want} value in return statement` (HYB-E1002)"
            ),
            "Go has no implicit conversion between typed numeric values; write an explicit conversion (`float64(n)`) when a conversion is intended",
        );
    }

    fn report_return_conflict(&mut self, val: &Expr, got: &OpKind, want: &OpKind, span: Span) -> bool {
        let is_untyped_int_to_float = *want == OpKind::Float
            && *got == OpKind::Int
            && self.is_untyped_int_const(val);
        let conflict = (*want == OpKind::Float && *got == OpKind::Int && !is_untyped_int_to_float)
            || kinds_conflict(want, got);
        if !conflict {
            return false;
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
        true
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
    /// #2796 — the destination element type of `append`'s first argument,
    /// canonicalised to its Go numeric name (`i64` → `int64`, `f32` →
    /// `float32`, `byte` → `uint8`) so it can be compared against
    /// `typed_value_scalar`. `None` when the spelling is not a predeclared
    /// numeric type, so user types and unknown shapes fail open.
    fn append_dest_scalar(&self, first: &Expr) -> Option<&'static str> {
        self.container_elem_scalar_of_expr(first).or_else(|| {
            self.slice_element_type_name(first)
                .as_deref()
                .and_then(canonical_numeric_name)
        })
    }

    fn check_append_elements(&mut self, args: &[CallArg]) {
        let Some(first) = args.first() else { return };
        let elem = match self.kind_of_expr(&first.value) {
            OpKind::Slice(e) => *e,
            _ => return,
        };
        for extra in &args[1..] {
            // `append(a, b...)` — a variadic spread passes the whole slice (or string
            // into `[]byte`). Go requires the spread operand's element type to match
            // the destination slice element type (issue #2738).
            if extra.spread {
                let got = self.kind_of_expr(&extra.value);
                match got {
                    OpKind::Unknown => {}
                    OpKind::Slice(extra_elem) => {
                        let dst_elem_ty = self.slice_element_type_name(&first.value);
                        let src_elem_ty = self.slice_element_type_name(&extra.value);
                        let ty_mismatch = match (&dst_elem_ty, &src_elem_ty) {
                            (Some(d), Some(s)) => elem_types_conflict(d, s),
                            _ => false,
                        };
                        let kind_mismatch = elem != *extra_elem
                            && !matches!(elem, OpKind::Unknown)
                            && !matches!(*extra_elem, OpKind::Unknown);
                        if kinds_conflict(&elem, &extra_elem) || kind_mismatch || ty_mismatch {
                            let (d_label, s_label) = match (dst_elem_ty, src_elem_ty) {
                                (Some(d), Some(s)) => (format!("[]{}", d), format!("[]{}", s)),
                                _ => (
                                    self.kind_of_expr(&first.value).label().to_string(),
                                    format!("[]{}", extra_elem.label()),
                                ),
                            };
                            self.emit_e1002(
                                extra.value.span(),
                                format!(
                                    "cannot use `{}` (type `{}`) as `{}` value in argument to append (HYB-E1002)",
                                    extra.value.render(),
                                    s_label,
                                    d_label
                                ),
                                "elements of the spread slice must match the destination slice element type",
                            );
                        }
                    }
                    OpKind::Str => {
                        let dst_elem_ty = self.slice_element_type_name(&first.value);
                        let is_byte_slice = matches!(elem, OpKind::Int)
                            && dst_elem_ty.as_deref().map_or(false, |t| {
                                matches!(crate::const_overflow::canonical_int_type_name(t), "uint8")
                            });
                        let cannot_be_byte = !matches!(elem, OpKind::Int | OpKind::Unknown)
                            || (matches!(elem, OpKind::Int)
                                && dst_elem_ty.as_deref().map_or(false, |t| {
                                    !matches!(
                                        crate::const_overflow::canonical_int_type_name(t),
                                        "uint8"
                                    )
                                }));
                        if cannot_be_byte && !is_byte_slice {
                            let d_label = dst_elem_ty
                                .map(|d| format!("[]{}", d))
                                .unwrap_or_else(|| self.kind_of_expr(&first.value).label().to_string());
                            self.emit_e1002(
                                extra.value.span(),
                                format!(
                                    "cannot use `{}` (type `string`) as `{}` value in argument to append (HYB-E1002)",
                                    extra.value.render(),
                                    d_label
                                ),
                                "string elements can only be appended to a byte slice (`[]byte`)",
                            );
                        }
                    }
                    _ => {
                        self.emit_e1002(
                            extra.value.span(),
                            format!(
                                "cannot use `{}` (type `{}`) as `{}` value in argument to append (HYB-E1002)",
                                extra.value.render(),
                                got.label(),
                                self.kind_of_expr(&first.value).label()
                            ),
                            "a spread argument to `append` must be a slice (or string for `[]byte`)",
                        );
                    }
                }
                continue;
            }
            let got = self.kind_of_expr(&extra.value);
            let dst_elem_ty = self.slice_element_type_name(&first.value);
            // #3518: reject untyped nil passed to non-nilable slice element types
            if Checker::is_nil_literal(&extra.value) && is_non_nilable(&elem) {
                let elem_label = match dst_elem_ty.as_deref() {
                    Some(d) => d.to_string(),
                    None => elem.label().to_string(),
                };
                self.emit_e1002(
                    extra.value.span(),
                    format!(
                        "cannot use nil as `{}` value in argument to append (HYB-E1002)",
                        elem_label
                    ),
                    "the appended values must be assignable to the slice's element type",
                );
                continue;
            }
            let int_to_float_conflict = elem == OpKind::Float
                && got == OpKind::Int
                && !self.is_untyped_int_const(&extra.value);
            // #2796: `OpKind` collapses every integer width into `Int` and
            // both float widths into `Float`, so a TYPED element of another
            // concrete numeric type (`int` into `[]int64`, `float32` into
            // `[]float64`) passed here and failed `go build` (`cannot use y
            // (variable of type int) as int64 value in argument to append`).
            let dst_elem_ty = self.slice_element_type_name(&first.value);
            let dst_scalar = self.append_dest_scalar(&first.value);
            let got_scalar = self.typed_value_scalar(&extra.value);
            let scalar_mismatch = matches!((dst_scalar, got_scalar), (Some(d), Some(g)) if d != g);
            let got_ty = self.typed_value_type_name(&extra.value);
            let elem_ty_mismatch = match (&dst_elem_ty, &got_ty) {
                (Some(d), Some(g)) => elem_types_conflict(d, g),
                _ => false,
            };
            if kinds_conflict(&elem, &got)
                || int_to_float_conflict
                || scalar_mismatch
                || elem_ty_mismatch
            {
                let dst_label = match dst_elem_ty.as_deref() {
                    Some(d) => format!("[]{}", d),
                    None => self.kind_of_expr(&first.value).label().to_string(),
                };
                let got_label = match got_scalar {
                    Some(s) => s.to_string(),
                    None => match got_ty.as_deref() {
                        Some(g) => g.to_string(),
                        None => got.label().to_string(),
                    },
                };
                let elem_label = match dst_scalar {
                    Some(d) => d.to_string(),
                    None => match dst_elem_ty.as_deref() {
                        Some(d) => d.to_string(),
                        None => elem.label().to_string(),
                    },
                };
                self.emit_e1002(
                    extra.value.span(),
                    format!(
                        "cannot append a value of type `{}` to a `{}` — Go refuses `cannot use ... as {} value in argument to append` (HYB-E1002)",
                        got_label,
                        dst_label,
                        elem_label
                    ),
                    "the appended values must be assignable to the slice's element type",
                );
            }
        }
    }

    /// #2817 — Go §Send statements: "the type of the value to be sent must be
    /// assignable to the channel's element type". `sendability.rs` treats every
    /// numeric kind as one coarse `Primitive`, and `Stmt::Send` only ran the
    /// #887 single-value check, so `ch <- f` with `f float64` on a `chan int`
    /// (or `int32` → `chan int64`, `float64` → `chan float32`) passed
    /// `hy-check` and failed `go build` with
    /// `cannot use f (variable of type float64) as int value in send` —
    /// a Row 13 BUILDABLE violation.
    ///
    /// Fail-open like the #682 U5 / #1031 precedents: only a PROVEN conflict is
    /// reported. Untyped constants stay legal (`ch <- 1` into `chan float64`
    /// builds), an unresolved channel element type or value type carries no
    /// claim, and `OpKind` width collapse is compensated by comparing the
    /// exact predeclared spellings whenever both sides are provably typed.
    fn check_send_assignability(&mut self, chan: &Expr, value: &Expr) {
        // O6.1: inside a `managed` block the hatch suppresses sendability
        // checking by contract — E8001 is the auditable record instead. The
        // #2817 rule must not reopen what the hatch closed (a top-level send
        // of a mismatched type still fires; only the hatch is respected).
        if self.managed_depth > 0 {
            return;
        }
        let want = match self.kind_of_expr(chan) {
            OpKind::Chan(e) => *e,
            _ => return,
        };
        let got = self.kind_of_expr(value);
        if want.is_numeric() && got.is_numeric() {
            if let (Some(w), Some(g)) =
                (self.chan_element_scalar(chan), self.typed_value_scalar(value))
            {
                if w != g {
                    self.emit_send_mismatch(value, g, w);
                }
                return;
            }
        }
        // A typed int value reaches a float channel through the (Float, Int)
        // arm of `kinds_conflict`, which must stay open for untyped int
        // constants; the same #682 U5 disambiguation applies here.
        let typed_int_to_float = want == OpKind::Float
            && got == OpKind::Int
            && !self.is_untyped_int_const(value);
        if kinds_conflict(&want, &got) || typed_int_to_float {
            self.emit_send_mismatch(value, &got.label(), &want.label());
        }
    }

    fn emit_send_mismatch(&mut self, value: &Expr, got: &str, want: &str) {
        self.emit_e1002(
            value.span(),
            format!(
                "cannot use `{}` (variable of type `{}`) as `{}` value in send (HYB-E1002)",
                value.render(),
                got,
                want
            ),
            "the sent value's type must be assignable to the channel's element type (Go §Send statements)",
        );
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

    fn check_match_lit_pattern(
        &mut self,
        subject: &Expr,
        subject_kind: &OpKind,
        lit: &LitKind,
        span: Span,
    ) {
        if matches!(lit, LitKind::Nil) && is_non_nilable(subject_kind) {
            self.emit_e1002(
                span,
                format!(
                    "cannot use match pattern `nil` on subject of non-nilable type `{}` — Go refuses `invalid case nil in switch on ... (mismatched types untyped nil and {})` (HYB-E1002)",
                    subject_kind.label(),
                    subject_kind.label(),
                ),
                "`nil` match pattern is only valid on nilable subjects (pointers, interfaces, channels, maps, slices, options)",
            );
            return;
        }

        if let LitKind::Int(raw) = lit {
            if let Some(scalar) = self.typed_value_scalar(subject) {
                if let Some((lo, hi)) = crate::const_overflow::fixed_width_int_range(scalar) {
                    let overflows = match crate::const_overflow::parse_int_lit(raw) {
                        Some(crate::const_overflow::ConstInt::Value(v)) => v < lo || v > hi,
                        Some(crate::const_overflow::ConstInt::PosOverflow) => true,
                        Some(crate::const_overflow::ConstInt::NegOverflow) => true,
                        None => false,
                    };
                    if overflows {
                        self.emit_e1002(
                            span,
                            format!(
                                "cannot use match pattern `{}` (untyped int constant) as `{}` value in case clause (overflows) (HYB-E1002)",
                                raw, scalar
                            ),
                            "match arm integer pattern must fit within the subject's declared integer range",
                        );
                        return;
                    }
                }
            }
        }

        let pat_kind = match lit {
            LitKind::Int(_) | LitKind::Char(_) => OpKind::Int,
            LitKind::Float(_) => OpKind::Float,
            LitKind::Bool(_) => OpKind::Bool,
            LitKind::Str(_) | LitKind::RawStr(_) => OpKind::Str,
            LitKind::Nil => OpKind::Unknown,
        };
        if kinds_conflict(subject_kind, &pat_kind) {
            let lit_str = match lit {
                LitKind::Int(s) | LitKind::Float(s) | LitKind::Str(s) | LitKind::RawStr(s) => s.as_str(),
                LitKind::Bool(true) => "true",
                LitKind::Bool(false) => "false",
                LitKind::Char(_) => "char",
                LitKind::Nil => "nil",
            };
            self.emit_e1002(
                span,
                format!(
                    "cannot use match pattern `{}` (type `{}`) on subject of type `{}` — Go refuses `invalid case ... in switch on ... (mismatched types)` (HYB-E1002)",
                    lit_str,
                    pat_kind.label(),
                    subject_kind.label(),
                ),
                "ensure match arm patterns match the subject expression's type",
            );
        }
    }

    /// Family A (#425, #3155): `if cond` and `for cond` require a boolean condition.
    fn check_bool_condition(&mut self, cond: &Expr, construct: &str) {
        self.check_single_value_position(cond, "a boolean condition", false);
        if matches!(unwrap_parens(cond), Expr::Lit { kind: LitKind::Nil, .. }) {
            self.emit_e1002(
                cond.span(),
                format!(
                    "non-boolean condition `nil` (untyped nil) in `{}` statement — Go requires a `bool` condition (HYB-E1002)",
                    construct
                ),
                "compare the value explicitly (e.g. `x != nil`) or pass a `bool` expression",
            );
            return;
        }
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
        // #1850: a unary operand is a single-value position. A void call or a
        // multi-value call here passed the checker and emitted
        // `!two_bools()` / `-two_ints()`, which Go rejects with
        // "multiple-value ... in single-value context" / "(no value) used as
        // value". Same check a binary operand already runs.
        self.check_single_value_position(operand, "a unary operand", /*binding_position=*/ false);
        // Issue #1777: `kind_of_expr` maps a nil literal to `Unknown`, and
        // `Unknown` is permissive in the gates below — so `!nil` and `-nil`
        // passed check and failed `go build`
        // (`invalid operation: operator ! not defined on nil`). `&nil` needs
        // no guard: the literal is not addressable (E1002 loc A).
        if matches!(op, UnOp::Not | UnOp::Neg) && Checker::is_nil_literal(operand) {
            let sym = op.render();
            self.emit_e1002(
                span,
                format!(
                    "invalid operation `{}{}`: nil is not a valid operand of `{}` (HYB-E1002)",
                    sym,
                    operand.render(),
                    sym
                ),
                "unary operators are not defined on `nil` — compare the nilable value explicitly instead",
            );
            return;
        }
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
            UnOp::Addr => {
                self.check_addr_of_operand(operand, span);
            }
        }
    }

    /// Go spec §Address operators: operand must be addressable or a composite literal.
    fn check_addr_of_operand(&mut self, operand: &Expr, span: Span) {
        if !self.is_addressable_expr(operand) {
            self.emit_e1002(
                span,
                format!(
                    "cannot take the address of `{}` (HYB-E1002)",
                    operand.render()
                ),
                "the operand of `&` must be addressable (a variable, pointer indirection, field, slice/array element, or composite literal)",
            );
        }
    }

    fn is_imported_pkg_root(&self, name: &str) -> bool {
        for d in &self.ck.file.decls {
            if let Decl::Import { path, .. } = d {
                let root = path.rsplit('/').next().unwrap_or(path.as_str());
                if root == name {
                    return true;
                }
            }
        }
        false
    }

    fn is_addressable_expr(&self, e: &Expr) -> bool {
        match unwrap_parens(e) {
            Expr::StructLit { .. } | Expr::SliceLit { .. } | Expr::MapLit { .. } => true,
            Expr::Ident { name, .. } => {
                if name == "_" {
                    return false;
                }
                for layer in self.scopes.iter().rev() {
                    if let Some(entry) = layer.get(name) {
                        return match entry {
                            Some(id) => self.defines[*id].kind != LocalKind::Const,
                            None => true,
                        };
                    }
                }
                if self.file_consts.contains_key(name) {
                    return false;
                }
                if self.ck.fns.contains_key(name) || self.ck.file.fns().any(|f| &f.name == name) {
                    return false;
                }
                if self.is_imported_pkg_root(name) {
                    return false;
                }
                true
            }
            Expr::Index { base, .. } => {
                let bk = self.kind_of_expr(base);
                match bk {
                    OpKind::Str => false,
                    OpKind::Map(_, _) => true,
                    OpKind::Slice(_) => true,
                    OpKind::Array(_) => self.is_addressable_expr(base),
                    OpKind::Pointer(inner) if matches!(*inner, OpKind::Array(_) | OpKind::Slice(_)) => true,
                    OpKind::Unknown => true,
                    _ => false,
                }
            }
            Expr::Field { base, .. } => {
                let bk = self.kind_of_expr(base);
                match bk {
                    OpKind::Pointer(_) | OpKind::Unknown => true,
                    _ => self.is_addressable_expr(base),
                }
            }
            Expr::Slice { .. } => false,
            Expr::Lit { .. }
            | Expr::Interp { .. }
            | Expr::Binary { .. }
            | Expr::Unary { .. }
            | Expr::AddrOf { .. }
            | Expr::Call { .. }
            | Expr::Try { .. }
            | Expr::Receive { .. }
            | Expr::Move { .. }
            | Expr::FnLit { .. } => false,
            Expr::Paren { inner, .. } => self.is_addressable_expr(inner),
        }
    }

    /// Family B3/B4/B5 (#425): binary operator operand-kind checks.
    fn check_binary_operands(&mut self, op: BinOp, lhs: &Expr, rhs: &Expr, span: Span) {
        // Issue #1777: a literal `nil` is not a valid operand of ANY operator
        // except the equality surface — and equality (`Eq`/`Ne`) is the
        // deliberate nil-comparison exception (E2006 returns early above).
        // `kind_of_expr` maps nil to `Unknown` because nil has no single
        // static type; `Unknown` is then permissive in every per-op gate
        // below, so `1 << nil`, `nil << 1`, `1 + nil`, `nil && false` all
        // sailed through to `go build` errors like
        // `cannot convert nil to type uint` / `untyped nil in shift`.
        // Ordered comparisons (`<` … `>=`) are already nil-closed
        // (nil-funnel rejections above) — leave them exactly where they are.
        if !matches!(
            op,
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
        ) && (Checker::is_nil_literal(lhs) || Checker::is_nil_literal(rhs))
        {
            let op_str = match op {
                BinOp::Add => "+",
                BinOp::Sub => "-",
                BinOp::Mul => "*",
                BinOp::Div => "/",
                BinOp::Mod => "%",
                BinOp::BitAnd => "&",
                BinOp::BitOr => "|",
                BinOp::BitXor => "^",
                BinOp::And => "&&",
                BinOp::Or => "||",
                BinOp::Shl => "<<",
                _ => ">>",
            };
            self.emit_e1002(
                span,
                format!(
                    "invalid operation `{}`: nil is not a valid operand of `{}` (HYB-E1002)",
                    span_expr_label(lhs, op_str, rhs),
                    op_str
                ),
                "`nil` only combines with `==` and `!=`, or as an assignment value for a nilable (error/foreign) type",
            );
            return;
        }
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
            // and comparisons between non-nilable types and nil are rejected
            // here under HYB-E1002 (rule `BUILDABLE` / SPEC §3.1 N1).
            BinOp::Eq | BinOp::Ne => {
                let op_str = if matches!(op, BinOp::Eq) { "==" } else { "!=" };
                let lhs_is_nil = Checker::is_nil_literal(lhs);
                let rhs_is_nil = Checker::is_nil_literal(rhs);
                if lhs_is_nil && rhs_is_nil {
                    self.emit_e1002(
                        span,
                        format!(
                            "invalid operation `{}`: operator `{}` not defined on untyped nil (HYB-E1002)",
                            span_expr_label(lhs, op_str, rhs),
                            op_str
                        ),
                        "untyped nil cannot be compared with untyped nil",
                    );
                } else if !lhs_is_nil && !rhs_is_nil {
                    // #2719: a typed integer and a typed float are different
                    // types, so Go refuses `a == b` (`mismatched types int and
                    // float64`). An untyped numeric constant adapts to the other
                    // operand (`int_var == 1.0` / `float_var == 1` build), so the
                    // conflict needs the side that must convert to be a genuine
                    // non-constant value. Mirrors the arithmetic and ordered-
                    // comparison arms above.
                    let int_float_conflict = (lk == OpKind::Float
                        && rk == OpKind::Int
                        && !self.is_untyped_int_const(rhs))
                        || (lk == OpKind::Int
                            && rk == OpKind::Float
                            && !self.is_untyped_int_const(lhs));
                    if equality_kinds_conflict(&lk, &rk)
                        || int_float_conflict
                        || self.scalar_conflict(lhs, rhs)
                        || self.sized_array_names_differ(
                            self.typed_value_type_name(lhs).as_deref(),
                            self.typed_value_type_name(rhs).as_deref(),
                        )
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
                } else if lhs_is_nil && is_non_nilable(&rk) {
                    self.emit_e1002(
                        span,
                        format!(
                            "invalid operation `{}`: operator `{}` not defined between untyped nil and `{}` (HYB-E1002)",
                            span_expr_label(lhs, op_str, rhs),
                            op_str,
                            rk.label()
                        ),
                        "operator `==` and `!=` cannot compare untyped nil to non-nilable types (SPEC §3.1 N1)",
                    );
                } else if rhs_is_nil && is_non_nilable(&lk) {
                    self.emit_e1002(
                        span,
                        format!(
                            "invalid operation `{}`: operator `{}` not defined between `{}` and untyped nil (HYB-E1002)",
                            span_expr_label(lhs, op_str, rhs),
                            op_str,
                            lk.label()
                        ),
                        "operator `==` and `!=` cannot compare untyped nil to non-nilable types (SPEC §3.1 N1)",
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
                let int_float_conflict = (lk == OpKind::Float && rk == OpKind::Int && !self.is_untyped_int_const(rhs))
                    || (lk == OpKind::Int && rk == OpKind::Float && !self.is_untyped_int_const(lhs));
                if !l_ok || !r_ok || mismatch || int_float_conflict || self.scalar_conflict(lhs, rhs) {
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
                let int_float_conflict = (lk == OpKind::Float && rk == OpKind::Int && !self.is_untyped_int_const(rhs))
                    || (lk == OpKind::Int && rk == OpKind::Float && !self.is_untyped_int_const(lhs));
                if !l_ok || !r_ok || mismatch || int_float_conflict || self.scalar_conflict(lhs, rhs) {
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
                let int_float_conflict = (lk == OpKind::Float && rk == OpKind::Int && !self.is_untyped_int_const(rhs))
                    || (lk == OpKind::Int && rk == OpKind::Float && !self.is_untyped_int_const(lhs));
                if !matches!(lk, OpKind::Unknown | OpKind::Int | OpKind::Float)
                    || !matches!(rk, OpKind::Unknown | OpKind::Int | OpKind::Float)
                    || int_float_conflict
                    || self.scalar_conflict(lhs, rhs)
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
                let is_shift = matches!(op, BinOp::Shl | BinOp::Shr);
                if !matches!(lk, OpKind::Unknown | OpKind::Int)
                    || !matches!(rk, OpKind::Unknown | OpKind::Int)
                    || (!is_shift && self.scalar_conflict(lhs, rhs))
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
                    let is_untyped_int_to_float = tk == OpKind::Float
                        && vk == OpKind::Int
                        && self.is_untyped_int_const(&values[0]);
                    let conflict = (tk == OpKind::Float && vk == OpKind::Int && !is_untyped_int_to_float)
                        || kinds_conflict(&tk, &vk);
                    // #2770: `g = f` between a `float64` and a `float32` local.
                    let scalar_mismatch = matches!(
                        (self.place_scalar(target), self.typed_value_scalar(&values[0])),
                        (Some(t), Some(v)) if t != v
                    );
                    // #2771: `b = a` between a `[4]int` and a `[3]int`.
                    if !is_loan_shape
                        && (conflict
                            || scalar_mismatch
                            || self.sized_array_names_differ(
                                self.place_type_name(target).as_deref(),
                                self.typed_value_type_name(&values[0]).as_deref(),
                            ))
                    {
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
                } else if values.len() > 1 && values.len() != targets.len() {
                    // Binding-count guards: a parallel assignment is
                    // positional — a count mismatch fails go build with
                    // `assignment mismatch`.
                    self.emit_e1002(
                        span,
                        format!(
                            "assignment mismatch: {} variables but {} values (HYB-E1002)",
                            targets.len(),
                            values.len()
                        ),
                        "a parallel assignment binds positionally; the target and value counts must match",
                    );
                } else if targets.len() > 1 && values.len() == targets.len() {
                    // Every positional pair must satisfy the same kind check
                    // as the single-target form, with the same O8 loan-shape
                    // fail-open.
                    for (t, v) in targets.iter().zip(values.iter()) {
                        if as_addr_of(v)
                            .is_some_and(|o| matches!(o, Expr::Index { .. } | Expr::Field { .. }))
                        {
                            continue;
                        }
                        let tpk = self.kind_of_place(t);
                        let vk = self.kind_of_expr(v);
                        let is_untyped_int_to_float = tpk == OpKind::Float
                            && vk == OpKind::Int
                            && self.is_untyped_int_const(v);
                        let conflict = (tpk == OpKind::Float && vk == OpKind::Int && !is_untyped_int_to_float)
                            || kinds_conflict(&tpk, &vk);
                        let scalar_mismatch = matches!(
                            (self.place_scalar(t), self.typed_value_scalar(v)),
                            (Some(tp), Some(vp)) if tp != vp
                        );
                        let array_mismatch = self.sized_array_names_differ(
                            self.place_type_name(t).as_deref(),
                            self.typed_value_type_name(v).as_deref(),
                        );
                        if conflict || scalar_mismatch || array_mismatch {
                            let (v_label, t_label) = if array_mismatch {
                                (
                                    self.typed_value_type_name(v)
                                        .unwrap_or_else(|| vk.label().to_string()),
                                    self.place_type_name(t)
                                        .unwrap_or_else(|| tpk.label().to_string()),
                                )
                            } else {
                                (vk.label().to_string(), tpk.label().to_string())
                            };
                            self.emit_e1002(
                                v.span(),
                                format!(
                                    "cannot assign a value of type `{}` to `{}` (type `{}`) in parallel assignment — Go refuses `cannot use ... as {} value in assignment` (HYB-E1002)",
                                    v_label,
                                    t.render(),
                                    t_label,
                                    t_label
                                ),
                                "each target's declared type is fixed; write an explicit conversion when a conversion is intended",
                            );
                        }
                    }
                } else if targets.len() > 1 && values.len() == 1 {
                    // One provably-single value bound to several targets —
                    // go build refuses with `assignment mismatch`. Call,
                    // receive, and index sources are owned by the
                    // source-count rules.
                    if is_definitely_single_value(&values[0]) {
                        self.emit_e1002(
                            span,
                            format!(
                                "assignment mismatch: {} variables but the value yields exactly 1 (HYB-E1002)",
                                targets.len()
                            ),
                            "bind one name per value, or use a source that yields one value per target",
                        );
                    } else {
                        let got_kinds = self.multi_value_kinds(&values[0]);
                        if got_kinds.len() == targets.len() {
                            for (t, vk) in targets.iter().zip(got_kinds.iter()) {
                                let tpk = self.kind_of_place(t);
                                if kinds_conflict(&tpk, vk) {
                                    self.emit_e1002(
                                        span,
                                        format!(
                                            "cannot assign a value of type `{}` to `{}` (type `{}`) in multi-value assignment — Go refuses `cannot use ... as {} value in assignment` (HYB-E1002)",
                                            vk.label(),
                                            t.render(),
                                            tpk.label(),
                                            tpk.label()
                                        ),
                                        "each target's declared type is fixed; write an explicit conversion when a conversion is intended",
                                    );
                                }
                            }
                        }
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
                // Issue #1777 (belt for the N1-silent shape): nil is
                // fail-open only in the `Unknown`-target gates — a literal
                // `nil` as the compound value is a `go build` error for
                // EVERY operator in this arm (`invalid operation: e += nil`).
                // Known-shaped targets already reject via E4001 (N1); here we
                // close the `Unknown`-target gap only, to stay layered.
                if tk == OpKind::Unknown && Checker::is_nil_literal(val) {
                    self.emit_e1002(
                        span,
                        format!(
                            "invalid compound assignment `{} {} {}`: nil is not a valid compound-assignment value (HYB-E1002)",
                            target.render(),
                            "+=",
                            val.render()
                        ),
                        "`nil` only combines with `==` and `!=`, or as an assignment value for a nilable (error/foreign) type",
                    );
                }
                let vk = self.kind_of_expr(val);
                let t_ok = matches!(tk, OpKind::Unknown | OpKind::Int | OpKind::Float | OpKind::Str);
                let v_ok = matches!(vk, OpKind::Unknown | OpKind::Int | OpKind::Float | OpKind::Str);
                let mismatch = tk != OpKind::Unknown
                    && vk != OpKind::Unknown
                    && tk.is_numeric() != vk.is_numeric();
                let is_untyped_int_to_float = tk == OpKind::Float
                    && vk == OpKind::Int
                    && self.is_untyped_int_const(val);
                let conflict = (tk == OpKind::Float && vk == OpKind::Int && !is_untyped_int_to_float)
                    || kinds_conflict(&tk, &vk);
                let scalar_mismatch = matches!(
                    (self.place_scalar(target), self.typed_value_scalar(val)),
                    (Some(tp), Some(vp)) if tp != vp
                );
                if !t_ok || !v_ok || mismatch || conflict || scalar_mismatch {
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
                if tk == OpKind::Unknown && Checker::is_nil_literal(val) {
                    let op_sym = match op {
                        AssignOp::SubAssign => "-=",
                        AssignOp::MulAssign => "*=",
                        _ => "/=",
                    };
                    self.emit_e1002(
                        span,
                        format!(
                            "invalid compound assignment `{} {} {}`: nil is not a valid compound-assignment value (HYB-E1002)",
                            target.render(),
                            op_sym,
                            val.render()
                        ),
                        "`nil` only combines with `==` and `!=`, or as an assignment value for a nilable (error/foreign) type",
                    );
                }
                let vk = self.kind_of_expr(val);
                let op_sym = match op {
                    AssignOp::SubAssign => "-=",
                    AssignOp::MulAssign => "*=",
                    _ => "/=",
                };
                let is_untyped_int_to_float = tk == OpKind::Float
                    && vk == OpKind::Int
                    && self.is_untyped_int_const(val);
                let conflict = (tk == OpKind::Float && vk == OpKind::Int && !is_untyped_int_to_float)
                    || kinds_conflict(&tk, &vk);
                let scalar_mismatch = matches!(
                    (self.place_scalar(target), self.typed_value_scalar(val)),
                    (Some(tp), Some(vp)) if tp != vp
                );
                if !matches!(tk, OpKind::Unknown | OpKind::Int | OpKind::Float)
                    || !matches!(vk, OpKind::Unknown | OpKind::Int | OpKind::Float)
                    || conflict
                    || scalar_mismatch
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
                if tk == OpKind::Unknown && Checker::is_nil_literal(val) {
                    let op_sym = match op {
                        AssignOp::ModAssign => "%=",
                        AssignOp::BitOrAssign => "|=",
                        AssignOp::BitAndAssign => "&=",
                        AssignOp::BitXorAssign => "^=",
                        AssignOp::ShlAssign => "<<=",
                        _ => ">>=",
                    };
                    self.emit_e1002(
                        span,
                        format!(
                            "invalid compound assignment `{} {} {}`: nil is not a valid compound-assignment value (HYB-E1002)",
                            target.render(),
                            op_sym,
                            val.render()
                        ),
                        "`nil` only combines with `==` and `!=`, or as an assignment value for a nilable (error/foreign) type",
                    );
                }
                let vk = self.kind_of_expr(val);
                let is_shift = matches!(op, AssignOp::ShlAssign | AssignOp::ShrAssign);
                let scalar_mismatch = !is_shift && matches!(
                    (self.place_scalar(target), self.typed_value_scalar(val)),
                    (Some(tp), Some(vp)) if tp != vp
                );
                if !matches!(tk, OpKind::Unknown | OpKind::Int)
                    || !matches!(vk, OpKind::Unknown | OpKind::Int)
                    || scalar_mismatch
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

    /// #3240: the declared key type spelling of a map-typed base.
    fn map_key_type_name(&self, base: Option<&Expr>) -> Option<String> {
        let base = base?;
        let ty = match unwrap_parens(base) {
            Expr::Ident { name, .. } => self
                .type_name_scopes
                .iter()
                .rev()
                .find_map(|l| l.get(name))
                .cloned()?,
            Expr::Field { base: s, name, .. } => {
                let bk = self.kind_of_expr(s);
                self.struct_field_type_name(&bk, name)?
            }
            _ => return None,
        };
        if let Some(CompFamily::Map { key, .. }) = CompFamily::split(ty.trim()) {
            return Some(key.to_string());
        }
        None
    }

    /// #3292: the declared key scalar of a map-typed base, when it is a
    /// predeclared numeric spelling. Anything else (named type, unresolved)
    /// gives None and the caller fails open.
    fn map_key_scalar_name(&self, base: Option<&Expr>) -> Option<&'static str> {
        let key = self.map_key_type_name(base)?;
        canonical_numeric_name(&key)
    }

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
        self.check_single_value_position(index, "an index expression", /*binding_position=*/ false);
        if let OpKind::Map(key_kind, _) = bk {
            if **key_kind != OpKind::Unknown
                && *ik != OpKind::Unknown
            {
                let is_untyped_int_to_float = **key_kind == OpKind::Float
                    && *ik == OpKind::Int
                    && self.is_untyped_int_const(index);
                // #3292: `OpKind` collapses every integer width, so a
                // `map[int32]V` indexed by an `int64` compared equal. Compare
                // the declared key spelling against the index value's spelling;
                // either side unresolved fails open so an untyped constant or
                // a named alias still passes.
                let key_scalar = self.map_key_scalar_name(base_expr);
                let idx_scalar = self.typed_value_scalar(index);
                let width_conflict = matches!((key_scalar, idx_scalar),
                    (Some(a), Some(b)) if a != b);
                let conflict = (**key_kind == OpKind::Float && *ik == OpKind::Int && !is_untyped_int_to_float)
                    || kinds_conflict(key_kind, ik)
                    || width_conflict;
                if conflict {
                    // #3292: prefer the declared spellings over the kind
                    // labels, which collapse int32/int64 to `int` and make
                    // the message read as a mismatch between identical types.
                    let key_label = key_scalar
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| key_kind.label());
                    let idx_label = idx_scalar
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| ik.label());
                    self.emit_e1002(
                        index.span(),
                        format!(
                            "cannot use `{}` (type `{}`) as `{}` value in map index (HYB-E1002)",
                            index.render(),
                            idx_label,
                            key_label
                        ),
                        "map index expression must match the map's key type",
                    );
                }
            }
            return;
        }
        // For slices, arrays, and strings, the index must be an integer.
        let needs_int_idx = matches!(bk, OpKind::Slice(_) | OpKind::Array(_) | OpKind::Str)
            || matches!(bk, OpKind::Pointer(inner) if matches!(**inner, OpKind::Array(_)));
        // A fixed-size array carries its length in the type, so a constant index
        // past that length is a compile-time error in Go ([3] on [2]int).
        let is_array_kind = matches!(bk, OpKind::Array(_))
            || matches!(bk, OpKind::Pointer(inner) if matches!(**inner, OpKind::Array(_)));
        let const_arr_len = if is_array_kind || matches!(bk, OpKind::Unknown) {
            base_expr.and_then(|b| self.eval_const_array_len_at(b, span.start))
        } else {
            None
        };
        let is_array = is_array_kind || const_arr_len.is_some();
        let needs_int_idx = needs_int_idx || is_array;
        if needs_int_idx && !matches!(ik, OpKind::Unknown | OpKind::Int | OpKind::Void) {
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
                } else if is_array {
                    if let Some(arr_len) = const_arr_len {
                        if v >= arr_len {
                            self.emit_e1002(
                                index.span(),
                                format!(
                                    "invalid array index `{}` (out of bounds for a {}-element array) (HYB-E1002)",
                                    index.render(),
                                    arr_len
                                ),
                                "a constant index on a fixed-size array must satisfy `0 <= index < len(a)`",
                            );
                        }
                    }
                }
            }
        }
    }

    /// #2789: whether an array-typed operand is NOT addressable in Go:
    /// a composite literal, a call or conversion result, a map index
    /// expression, or an element/field of such a value. Variables, slice
    /// elements and anything reached through a pointer are addressable.
    fn array_operand_unaddressable(&self, e: &Expr) -> bool {
        match unwrap_parens(e) {
            Expr::SliceLit { .. } | Expr::Call { .. } => true,
            Expr::Index { base, .. } => match self.kind_of_expr(base) {
                OpKind::Map(_, _) => true,
                OpKind::Array(_) => self.array_operand_unaddressable(base),
                _ => false,
            },
            Expr::Field { base, .. } => match self.kind_of_expr(base) {
                OpKind::Struct(_) => self.array_operand_unaddressable(base),
                _ => false,
            },
            _ => false,
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
        // #2789: Go spec, Slice expressions — "If the sliced operand is an
        // array, it must be addressable." `[3]int{..}[0:2]`, `arr()[1:]` and
        // `m["a"][:2]` (map of arrays) passed `hyb check` and failed `go
        // build` (`cannot slice unaddressable value`). Slices, strings and
        // pointers to arrays are unaffected.
        if matches!(bk, OpKind::Array(_)) && self.array_operand_unaddressable(base) {
            self.emit_e1002(
                span,
                format!(
                    "cannot slice unaddressable array value `{}` — Go refuses `cannot slice unaddressable value` (HYB-E1002)",
                    base.render()
                ),
                "only an addressable array can be sliced: bind the array to a variable first (`a := ...; a[lo:hi]`)",
            );
            return;
        }
        let const_str_len = if matches!(bk, OpKind::Str) {
            self.eval_const_string_len_at(base, span.start)
        } else {
            None
        };
        let is_array = matches!(bk, OpKind::Array(_))
            || matches!(bk, OpKind::Pointer(inner) if matches!(**inner, OpKind::Array(_)));
        let const_arr_len = if is_array || matches!(bk, OpKind::Unknown) {
            self.eval_const_array_len_at(base, span.start)
        } else {
            None
        };
        let mut lo_const: Option<i128> = None;
        let mut hi_const: Option<i128> = None;
        for (idx, bound_opt) in [lo, hi].into_iter().enumerate() {
            let Some(bound) = bound_opt else { continue };
            self.check_single_value_position(bound, "a slice bound", /*binding_position=*/ false);
            let k = self.kind_of_expr(bound);
            if !matches!(k, OpKind::Unknown | OpKind::Int | OpKind::Void) {
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
                    if let Some(arr_len) = const_arr_len {
                        if v > arr_len {
                            self.emit_e1002(
                                bound.span(),
                                format!(
                                    "invalid slice index `{}` (out of bounds for a {}-element array) (HYB-E1002)",
                                    bound.render(),
                                    arr_len
                                ),
                                "a constant slice bound on a fixed-size array must satisfy `0 <= bound <= len(a)`",
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

    fn is_untyped_int_const(&self, expr: &Expr) -> bool {
        match unwrap_parens(expr) {
            Expr::Lit {
                kind: LitKind::Int(_) | LitKind::Char(_),
                ..
            } => true,
            Expr::Ident { name, .. } => {
                for layer in self.scopes.iter().rev() {
                    if let Some(Some(id)) = layer.get(name) {
                        let d = &self.defines[*id];
                        if d.kind == LocalKind::Const && d.is_untyped {
                            return self.lookup_kind(name) == Some(OpKind::Int);
                        }
                        return false;
                    }
                }
                if self.file_untyped_consts.contains(name) {
                    return self.file_consts.get(name) == Some(&OpKind::Int);
                }
                false
            }
            Expr::Unary { op, operand, .. } => {
                matches!(op, UnOp::Neg) && self.is_untyped_int_const(operand)
            }
            Expr::Binary { op, lhs, rhs, .. } => {
                matches!(
                    op,
                    BinOp::Add
                        | BinOp::Sub
                        | BinOp::Mul
                        | BinOp::Div
                        | BinOp::Mod
                        | BinOp::BitAnd
                        | BinOp::BitOr
                        | BinOp::BitXor
                        | BinOp::Shl
                        | BinOp::Shr
                ) && self.is_untyped_int_const(lhs)
                    && self.is_untyped_int_const(rhs)
            }
            _ => false,
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

    /// Fixed-size array length, when `e` denotes an array whose length is known
    /// exactly. Unlike the constant folders this deliberately skips
    /// `expr_has_non_const_local`: an array's length lives in its type, so a
    /// `var`/`let` binding (`var a = [3]int{}`) is just as certain as a `const`
    /// one, and the resolver never falls back to a same-named package constant.
    fn eval_const_array_len_at(&self, e: &Expr, use_pos: hy_diag::Pos) -> Option<i128> {
        self.ck.const_array_len_at(e, use_pos, 0)
    }

    fn check_array_lit_items(&mut self, elem_ty: &TypeExpr, items: &[Expr]) {
        for item in items {
            self.check_single_value_position(item, "an array literal element", false);
        }
        let resolved_ty = self.ck.resolve_type_alias(elem_ty);
        let expected = self.kind_of_ty(&resolved_ty, 0);
        for item in items {
            let bare = unwrap_parens(item);
            if matches!(bare, Expr::Lit { kind: LitKind::Nil, .. }) {
                continue;
            }
            if self.non_single_value_count(item).is_some() {
                continue;
            }
            if let Expr::SliceLit {
                elem: None,
                items: sub_items,
                ..
            } = bare
            {
                match &resolved_ty {
                    TypeExpr::Array { elem: sub_ty, .. } => {
                        self.check_array_lit_items(sub_ty, sub_items);
                        continue;
                    }
                    TypeExpr::Slice { elem: sub_ty, .. } => {
                        self.check_slice_lit_items(sub_ty, sub_items);
                        continue;
                    }
                    _ if matches!(expected, OpKind::Interface | OpKind::Unknown) => {
                        self.check_untyped_slice_lit_items(sub_items);
                        continue;
                    }
                    _ => {}
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
            if expected == OpKind::Unknown {
                continue;
            }
            let ik = self.kind_of_expr(item);
            // #2788: `ik == expected` collapses every integer width into
            // `Int` and both float widths into `Float`, so a TYPED element
            // whose concrete spelling differs (`int` into `int64`, `float64`
            // into `float32`) slipped through and failed `go build`
            // (`cannot use i (variable of type int) as int64 value in array
            // or slice literal`).
            let scalar_mismatch = self
                .scalar_names_conflict(self.scalar_of_ty(&resolved_ty), self.typed_value_scalar(item));
            // #3412: the kind model also collapses container inners
            // (`chan int64` and `chan int32` are both `Chan(Int)`), so a
            // typed element whose composite inner differs passed the equal
            // kind arm and failed `go build`. Deep-compare the spellings;
            // aliases and type parameters fail open.
            let deep_conflict = if ik == expected && expected != OpKind::Interface {
                self.deep_composite_conflict(&resolved_ty, self.typed_value_type_name(item).as_deref())
            } else {
                None
            };
            if ik == OpKind::Unknown || expected == OpKind::Interface {
                continue;
            }
            if ik == expected && !scalar_mismatch && deep_conflict.is_none() {
                continue;
            }
            let is_untyped_int_to_float = expected == OpKind::Float
                && ik == OpKind::Int
                && self.is_untyped_int_const(item);
            let conflict = (expected == OpKind::Float && ik == OpKind::Int && !is_untyped_int_to_float)
                || kinds_conflict(&expected, &ik)
                || scalar_mismatch
                || deep_conflict.is_some();
            if !conflict {
                continue;
            }
            let got_ty = if deep_conflict.is_some() {
                self.typed_value_type_name(item).unwrap_or_else(|| ik.label().to_string())
            } else {
                ik.label().to_string()
            };
            let detail = deep_conflict
                .map(|r| format!(" — {}", r))
                .unwrap_or_default();
            self.emit_e1002(
                item.span(),
                format!(
                    "cannot use `{}` (type `{}`) as `{}` value in array literal{} (HYB-E1002)",
                    item.render(),
                    got_ty,
                    elem_ty.render(),
                    detail
                ),
                "every element of an array composite literal `[N]T{...}` / `[...]T{...}` must be assignable to the element type `T`",
            );
        }
    }

    fn check_slice_lit_items(&mut self, elem_ty: &TypeExpr, items: &[Expr]) {
        for item in items {
            self.check_single_value_position(item, "a slice literal element", false);
        }
        let resolved_ty = self.ck.resolve_type_alias(elem_ty);
        let expected = self.kind_of_ty(&resolved_ty, 0);
        for item in items {
            let bare = unwrap_parens(item);
            if matches!(bare, Expr::Lit { kind: LitKind::Nil, .. }) {
                continue;
            }
            if self.non_single_value_count(item).is_some() {
                continue;
            }
            if let Expr::SliceLit {
                elem: None,
                items: sub_items,
                ..
            } = bare
            {
                match &resolved_ty {
                    TypeExpr::Array { elem: sub_ty, .. } => {
                        self.check_array_lit_items(sub_ty, sub_items);
                        continue;
                    }
                    TypeExpr::Slice { elem: sub_ty, .. } => {
                        self.check_slice_lit_items(sub_ty, sub_items);
                        continue;
                    }
                    _ if matches!(expected, OpKind::Interface | OpKind::Unknown) => {
                        self.check_untyped_slice_lit_items(sub_items);
                        continue;
                    }
                    _ => {}
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
            if expected == OpKind::Unknown {
                continue;
            }
            let ik = self.kind_of_expr(item);
            // #2788: a typed element of another concrete numeric width.
            let scalar_mismatch = self
                .scalar_names_conflict(self.scalar_of_ty(&resolved_ty), self.typed_value_scalar(item));
            // #3412: the kind model collapses container inners, so a typed
            // element whose composite inner differs passed the equal kind
            // arm. Deep-compare the spellings; aliases and type parameters
            // fail open.
            let deep_conflict = if ik == expected && expected != OpKind::Interface {
                self.deep_composite_conflict(&resolved_ty, self.typed_value_type_name(item).as_deref())
            } else {
                None
            };
            if ik == OpKind::Unknown || expected == OpKind::Interface {
                continue;
            }
            if ik == expected && !scalar_mismatch && deep_conflict.is_none() {
                continue;
            }
            if expected == OpKind::Float
                && ik == OpKind::Int
                && self.is_untyped_int_const(item)
            {
                continue;
            }
            let is_untyped_int_to_float = expected == OpKind::Float
                && ik == OpKind::Int
                && self.is_untyped_int_const(item);
            let conflict = (expected == OpKind::Float && ik == OpKind::Int && !is_untyped_int_to_float)
                || kinds_conflict(&expected, &ik)
                || scalar_mismatch
                || deep_conflict.is_some();
            if !conflict {
                continue;
            }
            let got_ty = if deep_conflict.is_some() {
                self.typed_value_type_name(item).unwrap_or_else(|| ik.label().to_string())
            } else {
                ik.label().to_string()
            };
            let detail = deep_conflict
                .map(|r| format!(" — {}", r))
                .unwrap_or_default();
            self.emit_e1002(
                item.span(),
                format!(
                    "cannot use `{}` (type `{}`) as `{}` value in slice literal{} (HYB-E1002)",
                    item.render(),
                    got_ty,
                    elem_ty.render(),
                    detail
                ),
                "every element of a slice composite literal `[]T{...}` must be assignable to the element type `T`",
            );
        }
    }

    /// `[h, a, b]` with no element type lowers to `[]H{h, a, b}`, where `H`
    /// is what hy-emit infers from the first item (`infer_slice_head`): `int`,
    /// `float64`, `string`, `bool`, or `rune` for a basic literal (a sign or
    /// parentheses are transparent), the struct type for a typed struct
    /// literal. Every later item must be assignable to `H`, exactly as in
    /// the spelled-out `[]H{...}` — before this check `[1, "a"]`,
    /// `[1, 2.5]`, `[1, x]` with `x float64`, and `["a", nil]` passed
    /// `hyb check` and failed `go build` (`cannot use … in array or slice
    /// literal`). A `nil` first item has no type at all and is refused by
    /// the emitter (`nil in an untyped slice literal`), so it is rejected
    /// here too.
    fn check_untyped_slice_lit_items(&mut self, items: &[Expr]) {
        let Some((first, rest)) = items.split_first() else {
            return;
        };
        if matches!(
            unwrap_parens(first),
            Expr::Lit {
                kind: LitKind::Nil,
                ..
            }
        ) {
            // `[nil, …]`: no element type at all — hy-emit refuses it
            // (`nil in an untyped slice literal`).
            self.emit_e1002(
                first.span(),
                "an untyped slice literal cannot start with `nil` — it has no element type to infer (HYB-E1002)".to_string(),
                "spell the element type: `[]*T{nil}`, `[]error{nil}`, …",
            );
            return;
        }
        let Some(head) = untyped_slice_head_ty(first) else {
            // `[x]`, `[f()]`, `[a + b]`, `[s.f]`, `[arr[i]]`: hy-emit's
            // `infer_slice_head` reads the element type from a basic literal,
            // a sign, parentheses, or a typed struct literal — nothing else.
            // A non-literal head used to be skipped here and then refused at
            // `hyb build` ("untyped slice literal of `x` — spell the element
            // type"), so `hyb check` accepted a program that could not be
            // emitted (GUARANTEES Row 13, BUILDABLE).
            self.emit_e1002(
                first.span(),
                format!(
                    "an untyped slice literal cannot infer its element type from `{}` — the emitter reads it only from a literal or a typed struct literal (HYB-E1002)",
                    first.render()
                ),
                "spell the element type: `[]int{x}`, `[]T{f()}`, `[]T{s.f}`, …",
            );
            return;
        };
        let head_text = head.render();
        let mut checked = Vec::new();
        for item in rest {
            match unwrap_parens(item) {
                Expr::Lit {
                    kind: LitKind::Nil,
                    ..
                } => {
                    self.emit_e1002(
                        item.span(),
                        format!(
                            "cannot use `nil` as `{}` value in the untyped slice literal (its element type is inferred from the first item `{}`) (HYB-E1002)",
                            head_text,
                            first.render()
                        ),
                        "an untyped `[...]` takes its element type from its first item; spell a nilable element type instead (`[]*T{...}`, `[]error{...}`)",
                    );
                }
                // Go accepts an integral float constant in an int slot
                // (`[]int{1, 2.0}`).
                Expr::Lit {
                    kind: LitKind::Float(f),
                    ..
                } if head_text == "int"
                    && f.replace('_', "").parse::<f64>().is_ok_and(|v| v.fract() == 0.0) => {}
                _ => checked.push(item.clone()),
            }
        }
        let before = self.diags.len();
        self.check_slice_lit_items(&head, &checked);
        for d in &mut self.diags[before..] {
            d.notes.push(format!(
                "why: the untyped slice literal's element type `{}` is inferred from its first item `{}`; spell the element type (`[]T{{...}}`) when the items differ",
                head_text,
                first.render()
            ));
        }
    }

    fn check_map_lit_entries(
        &mut self,
        key_ty: &TypeExpr,
        val_ty: &TypeExpr,
        entries: &[(Expr, Expr)],
    ) {
        for (k, v) in entries {
            self.check_single_value_position(k, "a map key", false);
            self.check_single_value_position(v, "a map value", false);
        }
        let resolved_key = self.ck.resolve_type_alias(key_ty);
        let exp_key = self.kind_of_ty(&resolved_key, 0);
        let resolved_val = self.ck.resolve_type_alias(val_ty);
        let exp_val = self.kind_of_ty(&resolved_val, 0);
        for (k, v) in entries {
            let mut key_delegated = false;
            if self.non_single_value_count(k).is_none() {
                if let Expr::SliceLit {
                    elem: None,
                    items: sub_items,
                    ..
                } = unwrap_parens(k)
                {
                    match &resolved_key {
                        TypeExpr::Array { elem: sub_ty, .. } => {
                            self.check_array_lit_items(sub_ty, sub_items);
                            key_delegated = true;
                        }
                        TypeExpr::Slice { elem: sub_ty, .. } => {
                            self.check_slice_lit_items(sub_ty, sub_items);
                            key_delegated = true;
                        }
                        _ if matches!(exp_key, OpKind::Interface | OpKind::Unknown) => {
                            self.check_untyped_slice_lit_items(sub_items);
                            key_delegated = true;
                        }
                        _ => {}
                    }
                }
            }
            if !key_delegated
                && exp_key != OpKind::Unknown
                && exp_key != OpKind::Interface
                && self.non_single_value_count(k).is_none()
            {
                let kk = self.kind_of_expr(k);
                // #2788: a typed key of another concrete numeric width.
                let scalar_mismatch = self
                    .scalar_names_conflict(self.scalar_of_ty(&resolved_key), self.typed_value_scalar(k));
                // #3412: the kind model collapses container inners, so a
                // typed key whose composite inner differs passed the equal
                // kind arm (`map[chan int64]string{...}` into `map[chan int32]string`).
                let deep_conflict = if kk == exp_key && exp_key != OpKind::Interface {
                self.deep_composite_conflict(&resolved_key, self.typed_value_type_name(k).as_deref())
            } else {
                None
            };
                let is_untyped_int_to_float = exp_key == OpKind::Float
                    && kk == OpKind::Int
                    && self.is_untyped_int_const(k);
                let conflict = (exp_key == OpKind::Float && kk == OpKind::Int && !is_untyped_int_to_float)
                    || kinds_conflict(&exp_key, &kk)
                    || scalar_mismatch
                    || deep_conflict.is_some();
                if kk != OpKind::Unknown && conflict {
                    let got_ty = if deep_conflict.is_some() {
                        self.typed_value_type_name(k).unwrap_or_else(|| kk.label().to_string())
                    } else {
                        kk.label().to_string()
                    };
                    let detail = deep_conflict
                        .map(|r| format!(" — {}", r))
                        .unwrap_or_default();
                    self.emit_e1002(
                        k.span(),
                        format!(
                            "cannot use `{}` (type `{}`) as `{}` value in map key{} (HYB-E1002)",
                            k.render(),
                            got_ty,
                            key_ty.render(),
                            detail
                        ),
                        "every key of a map composite literal `map[K]V{...}` must be assignable to the key type `K`",
                    );
                }
            }
            let mut value_delegated = false;
            if self.non_single_value_count(v).is_none() {
                if let Expr::SliceLit {
                    elem: None,
                    items: sub_items,
                    ..
                } = unwrap_parens(v)
                {
                    match &resolved_val {
                        TypeExpr::Array { elem: sub_ty, .. } => {
                            self.check_array_lit_items(sub_ty, sub_items);
                            value_delegated = true;
                        }
                        TypeExpr::Slice { elem: sub_ty, .. } => {
                            self.check_slice_lit_items(sub_ty, sub_items);
                            value_delegated = true;
                        }
                        _ if matches!(exp_val, OpKind::Interface | OpKind::Unknown) => {
                            self.check_untyped_slice_lit_items(sub_items);
                            value_delegated = true;
                        }
                        _ => {}
                    }
                }
            }
            if !value_delegated
                && exp_val != OpKind::Unknown
                && exp_val != OpKind::Interface
                && self.non_single_value_count(v).is_none()
            {
                let bare_v = unwrap_parens(v);
                if matches!(bare_v, Expr::Lit { kind: LitKind::Nil, .. }) {
                    continue;
                }
                let vk = self.kind_of_expr(v);
                // #2788: a typed value of another concrete numeric width.
                let scalar_mismatch = self
                    .scalar_names_conflict(self.scalar_of_ty(&resolved_val), self.typed_value_scalar(v));
                // #3412: the kind model collapses container inners, so a
                // typed value whose composite inner differs passed the equal
                // kind arm (`map[string]chan int64` into `map[string]chan int32`).
                let deep_conflict = if vk == exp_val && exp_val != OpKind::Interface {
                self.deep_composite_conflict(&resolved_val, self.typed_value_type_name(v).as_deref())
            } else {
                None
            };
                let is_untyped_int_to_float = exp_val == OpKind::Float
                    && vk == OpKind::Int
                    && self.is_untyped_int_const(v);
                let conflict = (exp_val == OpKind::Float && vk == OpKind::Int && !is_untyped_int_to_float)
                    || kinds_conflict(&exp_val, &vk)
                    || scalar_mismatch
                    || deep_conflict.is_some();
                if vk != OpKind::Unknown && conflict {
                    let got_ty = if deep_conflict.is_some() {
                        self.typed_value_type_name(v).unwrap_or_else(|| vk.label().to_string())
                    } else {
                        vk.label().to_string()
                    };
                    let detail = deep_conflict
                        .map(|r| format!(" — {}", r))
                        .unwrap_or_default();
                    self.emit_e1002(
                        v.span(),
                        format!(
                            "cannot use `{}` (type `{}`) as `{}` value in map value{} (HYB-E1002)",
                            v.render(),
                            got_ty,
                            val_ty.render(),
                            detail
                        ),
                        "every value of a map composite literal `map[K]V{...}` must be assignable to the value type `V`",
                    );
                }
            }
        }
    }

    /// Family D1 & Family E (#425): non-function call check + binding-shaped builtin arg checks.
    fn check_call_operands(&mut self, callee: &Expr, args: &[CallArg], span: Span) {
        let bare = unwrap_parens(callee);
        match bare {
            // `[]byte("ok")` / `[]rune(s)`: parser encodes slice and array type conversions as
            // `Call { callee: SliceLit { elem: Some(T), items: [] }, args: [x] }`.
            Expr::SliceLit {
                elem: Some(elem_ty),
                items,
                ..
            } if items.is_empty() => {
                if let Some((_, _, arr_ty)) = bare.as_array_lit() {
                    let target_str = arr_ty.render();
                    if args.len() != 1 {
                        self.emit_e1002(
                            span,
                            format!(
                                "a conversion to `{}` takes exactly 1 argument, found {} — Go refuses `too many arguments in conversion to {}` (HYB-E1002)",
                                target_str,
                                args.len(),
                                target_str
                            ),
                            "a type conversion has the form `T(x)`: exactly one argument",
                        );
                        return;
                    }
                    let arg = &args[0].value;
                    self.check_single_value_position(arg, "a type conversion", false);
                    let ak = self.kind_of_expr(arg);
                    if !matches!(ak, OpKind::Unknown | OpKind::Slice(_) | OpKind::Array(_)) {
                        self.emit_e1002(
                            arg.span(),
                            format!(
                                "cannot convert `{}` (type `{}`) to `{}` (HYB-E1002)",
                                arg.render(),
                                ak.label(),
                                target_str
                            ),
                            "array type conversions require a slice or array operand",
                        );
                    }
                    return;
                }
                let elem_str = elem_ty.render();
                let target_str = format!("[]{}", elem_str);
                if args.len() != 1 {
                    self.emit_e1002(
                        span,
                        format!(
                            "a conversion to `{}` takes exactly 1 argument, found {} — Go refuses `too many arguments in conversion to {}` (HYB-E1002)",
                            target_str,
                            args.len(),
                            target_str
                        ),
                        "a type conversion has the form `T(x)`: exactly one argument",
                    );
                    return;
                }
                let arg = &args[0].value;
                self.check_single_value_position(arg, "a type conversion", false);
                let ak = self.kind_of_expr(arg);
                let is_byte_or_rune = matches!(
                    elem_str.as_str(),
                    "byte" | "uint8" | "u8" | "rune" | "int32" | "i32"
                );
                if is_byte_or_rune {
                    if !matches!(ak, OpKind::Unknown | OpKind::Str | OpKind::Slice(_)) {
                        self.emit_e1002(
                            arg.span(),
                            format!(
                                "cannot convert `{}` (type `{}`) to `{}` (HYB-E1002)",
                                arg.render(),
                                ak.label(),
                                target_str
                            ),
                            "`[]byte` and `[]rune` conversions require a string or compatible slice operand",
                        );
                    }
                } else if !matches!(ak, OpKind::Unknown | OpKind::Slice(_)) {
                    self.emit_e1002(
                        arg.span(),
                        format!(
                            "cannot convert `{}` (type `{}`) to `{}` (HYB-E1002)",
                            arg.render(),
                            ak.label(),
                            target_str
                        ),
                        "slice type conversions require a string (for `[]byte`/`[]rune`) or compatible slice operand",
                    );
                }
                // #2725: a slice-to-slice conversion must keep the element type
                // identical (Go §Conversions). `conversion_kinds_conflict` already
                // has that arm for the *named* conversion path, but a slice-literal
                // conversion (`[]string(xs)`) reaches HERE instead, and the checks
                // above only verified the operand WAS a slice. So `[]string([]int)`,
                // `[]int([]float64)` and `[]bool([]int)` were accepted and failed
                // `go build` (`cannot convert ... to type ...`, Row 13 BUILDABLE).
                if let OpKind::Slice(arg_elem) = &ak {
                    let target_elem = self.kind_of_ty(elem_ty, 0);
                    if copy_element_kinds_conflict(&target_elem, arg_elem) {
                        self.emit_e1002(
                            arg.span(),
                            format!(
                                "cannot convert `{}` (type `{}`) to `{}` — Go refuses `cannot convert {} (variable of type {}) to type {}` (HYB-E1002)",
                                arg.render(),
                                ak.label(),
                                target_str,
                                arg.render(),
                                ak.label(),
                                target_str
                            ),
                            "a slice conversion requires the two slice types to have identical element types",
                        );
                    }
                }
            }
            Expr::Ident { name, .. } => {
                if let Some(k) = self.lookup_kind(name) {
                    match k {
                        // A local bound to a closure or fn-typed value carries
                        // its declared parameter count, so the call's arity is
                        // checkable (#577 C9).
                        OpKind::Fn(_, Some(ref want)) if want.len() != args.len() => {
                            self.report_local_arity(name, want.len(), args.len(), span)
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
                    {
                        if args.len() != 2 {
                            self.emit_e1002(
                                span,
                                format!(
                                    "`errors.wrap` takes 2 arguments `(err, message)`, got {} (HYB-E1002)",
                                    args.len()
                                ),
                                "write `errors.wrap(err, \"context\")` (SPEC-LANGUAGE E2)",
                            );
                        } else {
                            self.check_errors_wrap_operands(args);
                        }
                        if self.is_fmt_shadowed() {
                            self.emit_e1002(
                                span,
                                "cannot call `errors.wrap` because `fmt` is shadowed by an in-scope declaration (HYB-E1002)".to_string(),
                                "`errors.wrap` lowers to `fmt.Errorf`; rename the shadowing `fmt` declaration so the standard `fmt` package is accessible",
                            );
                        }
                    }
                    // #3598: `fmt.eprintln`, `fmt.eprint`, and `fmt.eprintf` lower to
                    // `fmt.Fprint*(os.Stderr, ...)` in `hy-emit`. When `os` is shadowed by
                    // an in-scope parameter, local binding, loop variable, closure argument,
                    // type parameter, or package-level declaration, emitted Go fails
                    // downstream compilation (Row 13 BUILDABLE).
                    if pkg == "fmt"
                        && self.lookup_kind(pkg).is_none()
                        && self.is_os_shadowed()
                    {
                        if matches!(name.as_str(), "eprint" | "eprintln" | "eprintf") {
                            self.emit_e1002(
                                span,
                                format!(
                                    "cannot call `fmt.{}` because `os` is shadowed by an in-scope declaration (HYB-E1002)",
                                    name
                                ),
                                "`fmt.eprintln`, `fmt.eprint`, and `fmt.eprintf` lower to `os.Stderr`; rename the shadowing `os` binding so `os.Stderr` is accessible",
                            );
                        } else if name == "print" && args.len() == 1 {
                            let arg_expr = unwrap_parens(&args[0].value);
                            let is_bytes = match arg_expr {
                                Expr::Ident { name: b, .. } => {
                                    self.lookup_type_name(b).as_deref() == Some("[]byte")
                                        || self.lookup_type_name(b).as_deref() == Some("[]u8")
                                        || self.slice_element_type_name(arg_expr).as_deref() == Some("byte")
                                        || self.slice_element_type_name(arg_expr).as_deref() == Some("u8")
                                }
                                other => {
                                    self.slice_element_type_name(other).as_deref() == Some("byte")
                                        || self.slice_element_type_name(other).as_deref() == Some("u8")
                                }
                            };
                            if is_bytes {
                                self.emit_e1002(
                                    span,
                                    "cannot call `fmt.print` on byte slice because `os` is shadowed by an in-scope declaration (HYB-E1002)".to_string(),
                                    "`fmt.print` on a byte slice lowers to `os.Stdout.Write`; rename the shadowing `os` binding so `os.Stdout` is accessible",
                                );
                            }
                        }
                    }
                    if pkg == "httptest"
                        && self.lookup_kind(pkg).is_none()
                        && matches!(name.as_str(), "get" | "post")
                    {
                        if self.is_pkg_symbol_shadowed("strings") {
                            self.emit_e1002(
                                span,
                                format!(
                                    "cannot call `httptest.{}` because `strings` is shadowed by an in-scope declaration (HYB-E1002)",
                                    name
                                ),
                                "`httptest.get` and `httptest.post` lower to `strings.TrimSuffix`; rename the shadowing `strings` declaration so the standard `strings` package is accessible",
                            );
                        }
                        if self.is_pkg_symbol_shadowed("http") {
                            self.emit_e1002(
                                span,
                                format!(
                                    "cannot call `httptest.{}` because `http` is shadowed by an in-scope declaration (HYB-E1002)",
                                    name
                                ),
                                "`httptest.get` and `httptest.post` lower to `http.NewRequest`; rename the shadowing `http` declaration so `net/http` is accessible",
                            );
                        }
                    }
                    if pkg == "time"
                        && self.lookup_kind(pkg).is_none()
                        && matches!(name.as_str(), "milliseconds" | "seconds" | "minutes" | "hours")
                    {
                        if args.len() != 1 {
                            self.emit_e1002(
                                span,
                                format!(
                                    "`time.{}` takes exactly 1 argument `(n)`, got {} (HYB-E1002)",
                                    name,
                                    args.len()
                                ),
                                &format!("write `time.{}(10)` (SPEC duration constructor)", name),
                            );
                        } else {
                            let bare = unwrap_parens(&args[0].value);
                            if matches!(
                                bare,
                                Expr::Lit {
                                    kind: LitKind::Str(_)
                                        | LitKind::RawStr(_)
                                        | LitKind::Bool(_)
                                        | LitKind::Char(_)
                                        | LitKind::Nil,
                                    ..
                                }
                            ) {
                                self.emit_e1002(
                                    span,
                                    format!(
                                        "`time.{}` requires a numeric argument, found non-numeric literal (HYB-E1002)",
                                        name
                                    ),
                                    &format!("pass an integer or numeric expression to `time.{}`", name),
                                );
                            }
                        }
                    }
                    if pkg == "os"
                        && self.lookup_kind(pkg).is_none()
                        && matches!(name.as_str(), "args" | "stdout" | "stderr" | "stdin")
                        && !args.is_empty()
                    {
                        self.emit_e1002(
                            span,
                            format!(
                                "`os.{}` takes no arguments, got {} (HYB-E1002)",
                                name,
                                args.len()
                            ),
                            &format!("write `os.{}` (package variable, not a function)", name),
                        );
                    }
                    if pkg == "hex" && self.lookup_kind(pkg).is_none() {
                        // Full encoding/hex call surface — every member the
                        // member table in foreign.rs blesses, with the arity
                        // of the Go name it lowers to (`to_go_name`: snake
                        // case -> CamelCase, e.g. `encode` -> EncodeToString,
                        // `decode_string` -> DecodeString). Checking only
                        // `encode` let every other member escape with any
                        // arity and blow up at `go build` instead of at
                        // `hyb check` (Row 13 BUILDABLE: reject fail-closed,
                        // HYB-E1002). Arities are the Go signatures in
                        // encoding/hex.
                        let expected: Option<usize> = match name.as_str() {
                            "encode" // EncodeToString(src) -> string
                            | "encode_to_string" // EncodeToString(src)
                            | "decode_string" // DecodeString(s) (2 results)
                            | "dump" // Dump(b) -> string
                            | "dumper" // Dumper(w)
                            | "encoded_len" // EncodedLen(n)
                            | "decoded_len" // DecodedLen(x)
                            | "new_encoder" // NewEncoder(w)
                            | "new_decoder" // NewDecoder(r)
                            => Some(1),
                            "decode" => Some(2), // Decode(dst, src)
                            _ => None,
                        };
                        if let Some(n) = expected {
                            if args.len() != n {
                                let go_name = {
                                    // mirror hy-emit's to_go_name without a
                                    // cross-crate dep (hy-check must not
                                    // depend on hy-emit).
                                    let mut out = String::new();
                                    for part in name.split('_') {
                                        if part.is_empty() {
                                            continue;
                                        }
                                        let mut cs = part.chars();
                                        if let Some(c) = cs.next() {
                                            out.extend(c.to_uppercase());
                                        }
                                        out.push_str(cs.as_str());
                                    }
                                    out
                                };
                                self.emit_e1002(
                                    span,
                                    format!(
                                        "`hex.{}` takes exactly {} argument{}, got {} (HYB-E1002)",
                                        name,
                                        n,
                                        if n == 1 { "" } else { "s" },
                                        args.len()
                                    ),
                                    &format!(
                                        "match the arity of the Go function this lowers to (`hex.{}`)",
                                        go_name
                                    ),
                                );
                            }
                        } else if matches!(name.as_str(), "err_length" | "invalid_byte_error") {
                            self.emit_e1002(
                                span,
                                format!(
                                    "`hex.{}` is not a function — it is a package-level value/type and cannot be called (HYB-E1002)",
                                    name
                                ),
                                &format!("reference `hex.{}` directly, do not call it", name),
                            );
                        }
                    }

                    if pkg == "filepath" && self.lookup_kind(pkg).is_none() {
                        self.check_filepath_package_call(name.as_str(), args, span);
                    }
                    if pkg == "path" && self.lookup_kind(pkg).is_none() {
                        self.check_path_package_call(name.as_str(), args, span);
                    }
                    if pkg == "url" && self.lookup_kind(pkg).is_none() {
                        self.check_url_package_call(name.as_str(), args, span);
                    }
                    if pkg == "sort" && self.lookup_kind(pkg).is_none() {
                        self.check_sort_package_call(name.as_str(), args, span);
                    }
                    if pkg == "strconv" && self.lookup_kind(pkg).is_none() {
                        self.check_strconv_package_call(name.as_str(), args, span);
                    }
                    if pkg == "sync" && self.lookup_kind(pkg).is_none() {
                        self.check_sync_package_call(name.as_str(), args, span);
                    }
                    if pkg == "fmt" && self.lookup_kind(pkg).is_none() {
                        let method = name.as_str();
                        if matches!(method, "printf" | "sprintf" | "eprintf" | "errorf") {
                            if args.is_empty() {
                                self.emit_e1002(
                                    span,
                                    format!(
                                        "`fmt.{}` requires at least 1 format string argument `(format[, ...])`, got 0 (HYB-E1002)",
                                        name
                                    ),
                                    &format!("write `fmt.{}(\"%s\", ...)`", name),
                                );
                            } else {
                                let bare = unwrap_parens(&args[0].value);
                                if matches!(
                                    bare,
                                    Expr::Lit {
                                        kind: LitKind::Int(_)
                                            | LitKind::Float(_)
                                            | LitKind::Bool(_)
                                            | LitKind::Char(_)
                                            | LitKind::Nil,
                                        ..
                                    }
                                ) {
                                    self.emit_e1002(
                                        span,
                                        format!(
                                            "`fmt.{}` requires a string format argument, found non-string literal (HYB-E1002)",
                                            name
                                        ),
                                        &format!("pass a string literal or expression as format to `fmt.{}`", name),
                                    );
                                } else if let Some(kind_label) =
                                    self.nonstring_format_kind(bare)
                                {
                                    self.emit_e1002(
                                        span,
                                        format!(
                                            "`fmt.{}` requires a string format argument, found non-string ({}) (HYB-E1002)",
                                            name, kind_label
                                        ),
                                        &format!("pass a string literal or expression as format to `fmt.{}`", name),
                                    );
                                }
                            }
                        } else if method == "fprintf" {
                            if args.len() < 2 {
                                self.emit_e1002(
                                    span,
                                    format!(
                                        "`fmt.fprintf` requires at least 2 arguments `(w, format[, ...])`, got {} (HYB-E1002)",
                                        args.len()
                                    ),
                                    "write `fmt.fprintf(w, \"%s\", ...)`",
                                );
                            } else {
                                let bare = unwrap_parens(&args[1].value);
                                if matches!(
                                    bare,
                                    Expr::Lit {
                                        kind: LitKind::Int(_)
                                            | LitKind::Float(_)
                                            | LitKind::Bool(_)
                                            | LitKind::Char(_)
                                            | LitKind::Nil,
                                        ..
                                    }
                                ) {
                                    self.emit_e1002(
                                        span,
                                        "`fmt.fprintf` requires a string format second argument, found non-string literal (HYB-E1002)".to_string(),
                                        "pass a string literal or expression as format to `fmt.fprintf`",
                                    );
                                } else if let Some(kind_label) =
                                    self.nonstring_format_kind(bare)
                                {
                                    self.emit_e1002(
                                        span,
                                        format!(
                                            "`fmt.fprintf` requires a string format second argument, found non-string ({}) (HYB-E1002)",
                                            kind_label
                                        ),
                                        "pass a string literal or expression as format to `fmt.fprintf`",
                                    );
                                }
                            }
                        } else if matches!(method, "fprint" | "fprintln") && args.is_empty() {
                            self.emit_e1002(
                                span,
                                format!(
                                    "`fmt.{}` requires at least 1 destination writer argument `(w[, ...])`, got 0 (HYB-E1002)",
                                    name
                                ),
                                &format!("write `fmt.{}(w, ...)`", name),
                            );
                        }
                    }
                    if pkg == "fmt" && self.lookup_kind(pkg).is_none() {
                        let method = name.as_str();
                        if method == "scanf" {
                            if args.is_empty() {
                                self.emit_e1002(
                                    span,
                                    "`fmt.scanf` requires at least 1 format string argument `(format[, ...])`, got 0 (HYB-E1002)".to_string(),
                                    "write `fmt.scanf(\"%s\", ...)`",
                                );
                            } else {
                                let bare = unwrap_parens(&args[0].value);
                                if matches!(
                                    bare,
                                    Expr::Lit {
                                        kind: LitKind::Int(_)
                                            | LitKind::Float(_)
                                            | LitKind::Bool(_)
                                            | LitKind::Char(_)
                                            | LitKind::Nil,
                                        ..
                                    }
                                ) {
                                    self.emit_e1002(
                                        span,
                                        "`fmt.scanf` requires a string format argument, found non-string literal (HYB-E1002)".to_string(),
                                        "pass a string literal or expression as format to `fmt.scanf`",
                                    );
                                }
                            }
                        } else if method == "sscanf" {
                            if args.len() < 2 {
                                self.emit_e1002(
                                    span,
                                    format!(
                                        "`fmt.sscanf` requires at least 2 arguments `(str, format[, ...])`, got {} (HYB-E1002)",
                                        args.len()
                                    ),
                                    "write `fmt.sscanf(str, \"%s\", ...)`",
                                );
                            } else {
                                let bare0 = unwrap_parens(&args[0].value);
                                if matches!(
                                    bare0,
                                    Expr::Lit {
                                        kind: LitKind::Int(_)
                                            | LitKind::Float(_)
                                            | LitKind::Bool(_)
                                            | LitKind::Char(_)
                                            | LitKind::Nil,
                                        ..
                                    }
                                ) {
                                    self.emit_e1002(
                                        span,
                                        "`fmt.sscanf` requires a string first argument `(str)`, found non-string literal (HYB-E1002)".to_string(),
                                        "pass a string expression or literal as the input string to `fmt.sscanf`",
                                    );
                                }
                                let bare1 = unwrap_parens(&args[1].value);
                                if matches!(
                                    bare1,
                                    Expr::Lit {
                                        kind: LitKind::Int(_)
                                            | LitKind::Float(_)
                                            | LitKind::Bool(_)
                                            | LitKind::Char(_)
                                            | LitKind::Nil,
                                        ..
                                    }
                                ) {
                                    self.emit_e1002(
                                        span,
                                        "`fmt.sscanf` requires a string format second argument `(format)`, found non-string literal (HYB-E1002)".to_string(),
                                        "pass a string literal or expression as format to `fmt.sscanf`",
                                    );
                                }
                            }
                        } else if method == "fscanf" {
                            if args.len() < 2 {
                                self.emit_e1002(
                                    span,
                                    format!(
                                        "`fmt.fscanf` requires at least 2 arguments `(r, format[, ...])`, got {} (HYB-E1002)",
                                        args.len()
                                    ),
                                    "write `fmt.fscanf(r, \"%s\", ...)`",
                                );
                            } else {
                                let bare1 = unwrap_parens(&args[1].value);
                                if matches!(
                                    bare1,
                                    Expr::Lit {
                                        kind: LitKind::Int(_)
                                            | LitKind::Float(_)
                                            | LitKind::Bool(_)
                                            | LitKind::Char(_)
                                            | LitKind::Nil,
                                        ..
                                    }
                                ) {
                                    self.emit_e1002(
                                        span,
                                        "`fmt.fscanf` requires a string format second argument `(format)`, found non-string literal (HYB-E1002)".to_string(),
                                        "pass a string literal or expression as format to `fmt.fscanf`",
                                    );
                                }
                            }
                        } else if matches!(method, "sscan" | "sscanln") {
                            if args.is_empty() {
                                self.emit_e1002(
                                    span,
                                    format!(
                                        "`fmt.{}` requires at least 1 string argument `(str[, ...])`, got 0 (HYB-E1002)",
                                        name
                                    ),
                                    &format!("write `fmt.{}(str, ...)`", name),
                                );
                            } else {
                                let bare0 = unwrap_parens(&args[0].value);
                                if matches!(
                                    bare0,
                                    Expr::Lit {
                                        kind: LitKind::Int(_)
                                            | LitKind::Float(_)
                                            | LitKind::Bool(_)
                                            | LitKind::Char(_)
                                            | LitKind::Nil,
                                        ..
                                    }
                                ) {
                                    self.emit_e1002(
                                        span,
                                        format!(
                                            "`fmt.{}` requires a string first argument `(str)`, found non-string literal (HYB-E1002)",
                                            name
                                        ),
                                        &format!("pass a string expression or literal as the input string to `fmt.{}`", name),
                                    );
                                }
                            }
                        } else if matches!(method, "fscan" | "fscanln") && args.is_empty() {
                            self.emit_e1002(
                                span,
                                format!(
                                    "`fmt.{}` requires at least 1 source reader argument `(r[, ...])`, got 0 (HYB-E1002)",
                                    name
                                ),
                                &format!("write `fmt.{}(r, ...)`", name),
                            );
                        }
                    }

                    if pkg == "io" && self.lookup_kind(pkg).is_none() {
                        self.check_io_call_buildable(name, args, span);
                    }

                    if self.lookup_kind(pkg).is_none() {
                        let head = Checker::base_named_type_str(name);
                        let assoc_key = format!("{}.{}", pkg, head);
                        self.check_fn_type_arg_arity(&assoc_key, name, span);
                        self.check_user_fn_arity(&assoc_key, args, span);
                        self.check_user_fn_arg_types(&assoc_key, args);
                    }
                }
                if let Some(m) = self.method_expr_decl(base, name) {
                    self.check_method_expr_call(base, name, m, args, span);
                    return;
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
                        if let OpKind::Fn(_, Some(ref want)) = fk {
                            if want.len() != args.len() {
                                self.report_local_arity(&callee.render(), want.len(), args.len(), span);
                            }
                        }
                    }
                } else if name == "pop" && !has_method {
                    self.check_slice_pop_operands(base, &bk, args, span);
                    // `pop` is also a built-in slice operation, so this branch
                    // is entered before the missing-member check below. A
                    // struct, pointer, or interface only has `.pop()` if it
                    // declares the method; without one Go refuses the call with
                    // "<base>.pop undefined (type ... has no field or method pop)".
                    if matches!(
                        bk,
                        OpKind::Struct(_) | OpKind::Pointer(_) | OpKind::Interface
                    ) {
                        self.emit_e1002(
                            span,
                            format!(
                                "`{}` has no field or method `pop` (HYB-E1002)",
                                base.render()
                            ),
                            "a struct, pointer, or interface supports `.pop()` only when it declares a `pop` method; `pop` is a built-in only on slices",
                        );
                    }
                } else if !has_method {
                    if let Some(sn) = self.struct_base_name(&bk) {
                        let assoc_key = format!("{}.{}", sn, name);
                        if self.ck.fns.contains_key(&assoc_key) {
                            self.emit_e1002(
                                span,
                                format!(
                                    "`{}` has no field or method `{}` (type `{}`) — `{}` is an associated function, not a method (HYB-E1002)",
                                    base.render(),
                                    name,
                                    bk.label(),
                                    assoc_key
                                ),
                                "declare a method with a receiver, or call the associated function through its type (SPEC-LANGUAGE LX11)",
                            );
                            return;
                        }
                    }
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
                    // prelude call rather than a Go method (only on slices).
                    let is_lowered = matches!(bk, OpKind::Slice(_))
                        && LOWERED_METHOD_INTRINSICS.contains(&name.as_str());
                    let needle = format!("{}.{} undefined", base.render(), name);
                    if is_fully_resolved(&bk) && !is_lowered
                        && !self.diags.iter().any(|d| d.code == "HYB-E1002" && d.message.contains(&needle))
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
            Expr::FnLit { params, .. } => {
                self.check_fn_lit_call(params, args, callee, span);
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

    /// #3598: `fmt.eprintln`, `fmt.eprint`, and `fmt.eprintf` lower to
    /// `fmt.Fprint*(os.Stderr, ...)` in `hy-emit`. When `os` is shadowed by
    /// an in-scope parameter, local binding, loop variable, closure argument,
    /// type parameter, or package-level declaration, emitted Go resolves `os.Stderr`
    /// to the shadowing entity, causing downstream compilation to fail (Row 13 BUILDABLE).
    fn is_pkg_symbol_shadowed(&self, pkg: &str) -> bool {
        self.scopes.iter().any(|s| s.contains_key(pkg))
            || self.fn_type_params.contains(pkg)
            || self.lookup_kind(pkg).is_some()
            || self.ck.pkg_names.contains_key(pkg)
            || self.file_consts.contains_key(pkg)
            || self.file_untyped_consts.contains(pkg)
            || self.ck.structs.contains_key(pkg)
            || self.ck.fns.contains_key(pkg)
            || self.ck.type_aliases.contains_key(pkg)
            || self.ck.enums.contains_key(pkg)
    }

    fn is_os_shadowed(&self) -> bool {
        self.is_pkg_symbol_shadowed("os")
    }

    fn is_filepath_shadowed(&self) -> bool {
        self.is_pkg_symbol_shadowed("filepath")
    }

    fn is_fmt_shadowed(&self) -> bool {
        self.is_pkg_symbol_shadowed("fmt")
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
                } else if ptr_recv {
                    // #1480 Form C: a range-element copy IS addressable (Go
                    // emits `(&x).Bump()` on the per-iteration local), so the
                    // non-addressable rule above cannot catch it — the call
                    // builds and the mutation is silently dropped instead.
                    if let Expr::Ident { name: var, .. } = unwrap_parens(base) {
                        if self.is_copy_semantic_range_var(var) {
                            self.emit_e1002(
                                span,
                                format!(
                                    "cannot call pointer-receiver method `{}` on range element `{}` — `{}` is a per-iteration copy, so the mutation is silently dropped (HYB-E1002)",
                                    name,
                                    base.render(),
                                    base.render()
                                ),
                                "call it through the collection index instead: `for i in 0..len(xs) { xs[i].method() }`, or iterate a slice of pointers and call through the pointer",
                            );
                        }
                    }
                }
            }
        } else if let Expr::Ident { name: tn, .. } = unwrap_parens(base) {
            // #3142: Go §Method expressions: the method set of a defined type T
            // contains only methods declared with value receiver T. A method
            // declared with receiver `*T` requires a pointer receiver (`(*T).m`);
            // referencing or calling it as `T.m` is rejected by Go (`invalid method
            // expression T.m (needs pointer receiver (*T).m)`).
            if self.lookup_kind(tn).is_none() && !self.scopes.iter().any(|s| s.contains_key(tn)) {
                let base_sn = Checker::base_named_type_str(tn);
                if let Some(m) = self
                    .ck
                    .method_decls
                    .get(&(base_sn.to_string(), head.to_string()))
                {
                    if m.receiver.as_ref().is_some_and(|r| r.is_pointer) {
                        self.emit_e1002(
                            span,
                            format!(
                                "invalid method expression `{}.{}` — `{}` has a pointer receiver `*{}`; Go refuses `invalid method expression {}.{} (needs pointer receiver (*{}).{})` (HYB-E1002)",
                                tn, name, name, tn, tn, name, tn, name
                            ),
                            "a method with a pointer receiver cannot be called or bound as a value-type method expression: call it on an addressable value or pointer",
                        );
                    }
                }
            }
        }
    }

    fn check_field_selector(&mut self, base: &Expr, name: &str, span: Span) {
        if let Expr::Ident { name: ident_name, .. } = unwrap_parens(base) {
            if self.is_imported_pkg_root(ident_name)
                || self.ck.enums.contains_key(ident_name.as_str())
                || self.ck.structs.contains_key(ident_name.as_str())
                || self.ck.type_aliases.contains_key(ident_name.as_str())
                || self.ck.struct_type_params.contains_key(ident_name.as_str())
            {
                return;
            }
        }
        let bk = self.kind_of_expr(base);
        self.check_field_selector_kind(&base.render(), name, &bk, span);
    }

    fn check_field_selector_kind(&mut self, base_render: &str, name: &str, bk: &OpKind, span: Span) {
        if !is_fully_resolved(bk) || matches!(bk, OpKind::Interface) {
            return;
        }
        let is_lowered = matches!(bk, OpKind::Slice(_)) && LOWERED_METHOD_INTRINSICS.contains(&name);
        if is_lowered {
            return;
        }
        let is_option_method = match bk {
            OpKind::Option(_) => matches!(name, "is_some" | "is_none" | "unwrap" | "unwrap_or" | "expect"),
            _ => false,
        };
        if is_option_method {
            return;
        }

        let has_field = self.struct_field_kind(bk, name).is_some();
        let sn_opt = self.struct_base_name(bk);
        let has_method = sn_opt
            .as_ref()
            .is_some_and(|sn| self.ck.method_decls.contains_key(&(sn.clone(), name.to_string())));

        let is_struct_like = matches!(bk, OpKind::Struct(_) | OpKind::Pointer(_));
        let struct_known = if is_struct_like {
            sn_opt.as_ref().is_some_and(|sn| self.ck.structs.contains_key(sn))
        } else {
            false
        };

        if (!is_struct_like || struct_known) && !has_field && !has_method {
            let needle = format!("{}.{} undefined", base_render, name);
            if self.diags.iter().any(|d| (d.span == span && d.code == "HYB-E1002") || (d.code == "HYB-E1002" && d.message.contains(&needle))) {
                return;
            }
            self.emit_e1002(
                span,
                format!(
                    "`{}` has no field or method `{}` (type `{}`) — Go refuses `{}.{} undefined` (HYB-E1002)",
                    base_render,
                    name,
                    bk.label(),
                    base_render,
                    name
                ),
                "declare the field on the struct, or declare a method on that type",
            );
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

    /// #3572: `errors.wrap(err, message)` lowers to `fmt.Errorf("%s: %w", message, err)`.
    /// Reject operands whose resolved kind can never be an `error` (argument 0) or
    /// a `string` (argument 1) with HYB-E1002 (SPEC-LANGUAGE E2).
    fn check_errors_wrap_operands(&mut self, args: &[CallArg]) {
        let ek = self.kind_of_expr(&args[0].value);
        if matches!(
            ek,
            OpKind::Int
                | OpKind::Float
                | OpKind::Bool
                | OpKind::Str
                | OpKind::Slice(_)
                | OpKind::Array(_)
                | OpKind::Map(..)
                | OpKind::Chan(_)
                | OpKind::Fn(..)
                | OpKind::Option(_)
                | OpKind::Void
        ) {
            self.emit_e1002(
                args[0].value.span(),
                format!(
                    "`errors.wrap` needs an `error` as its first argument, got `{}` (HYB-E1002)",
                    ek.label()
                ),
                "write `errors.wrap(err, \"context\")` with an error value first (SPEC-LANGUAGE E2)",
            );
        }
        let mk = self.kind_of_expr(&args[1].value);
        if matches!(
            mk,
            OpKind::Int
                | OpKind::Float
                | OpKind::Bool
                | OpKind::Slice(_)
                | OpKind::Array(_)
                | OpKind::Map(..)
                | OpKind::Chan(_)
                | OpKind::Fn(..)
                | OpKind::Option(_)
                | OpKind::Void
        ) {
            self.emit_e1002(
                args[1].value.span(),
                format!(
                    "`errors.wrap` needs a `string` message as its second argument, got `{}` (HYB-E1002)",
                    mk.label()
                ),
                "write `errors.wrap(err, \"context\")` with a string context message (SPEC-LANGUAGE E2)",
            );
        }
    }

    /// Family E1–E5 (#425): extend `HYB-E2005` (`BUILTIN-ARGS`) from literals
    /// (`builtins.rs`) to resolved binding shapes.
    fn check_builtin_operand_kinds(&mut self, name: &str, args: &[CallArg], span: Span) {
        // #2701: among the built-ins only `append` is variadic; every other
        // built-in rejects the `...` spread form (`invalid use of ... with
        // built-in <name>`). `append` is handled in its own arm below, where a
        // spread argument is legal.
        if matches!(
            name,
            "len" | "cap" | "close" | "delete" | "min" | "max" | "clear" | "copy"
                | "real" | "imag" | "complex"
        ) && args.iter().any(|a| a.spread)
        {
            self.emit_e1002(
                span,
                format!(
                    "cannot use `...` in call to non-variadic built-in `{}` — Go refuses `invalid use of ... with built-in {}` (HYB-E1002)",
                    name, name
                ),
                "`...` (variadic spread) is only allowed on the variadic built-in `append`",
            );
            return;
        }
        match name {
            "make" => {
                for arg in args.iter().skip(1) {
                    if matches!(unwrap_parens(&arg.value), Expr::Lit { .. }) {
                        continue;
                    }
                    let ak = self.kind_of_expr(&arg.value);
                    if ak != OpKind::Unknown && ak != OpKind::Int {
                        self.emit_e1002(
                            arg.value.span(),
                            format!(
                                "non-integer size argument `{}` (type `{}`) in `make` — Go refuses `cannot use ... as int value in make` (HYB-E1002)",
                                arg.value.render(),
                                ak.label()
                            ),
                            "size arguments in `make` (len, cap, channel buffer size) must be integer types",
                        );
                    }
                }
            }
            "len" => {
                if args.len() != 1 {
                    return; // arity handled by builtins.rs
                }
                let op = &args[0].value;
                if builtins_already_flags_len_cap(op, false) {
                    return;
                }
                if let Some(tp) = self.expr_type_param(op) {
                    self.emit_e2005(
                        span,
                        format!(
                            "invalid argument `{}` (type `{}`) for built-in `len` (HYB-E2005)",
                            op.render(),
                            tp
                        ),
                        "`len` accepts a string, slice, array, map, or channel",
                    );
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
                if let Some(tp) = self.expr_type_param(op) {
                    self.emit_e2005(
                        span,
                        format!(
                            "invalid argument `{}` (type `{}`) for built-in `cap` (HYB-E2005)",
                            op.render(),
                            tp
                        ),
                        "`cap` accepts a slice, array, or channel — maps and strings have no capacity",
                    );
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
                if let Some(tp) = self.expr_type_param(op) {
                    self.emit_e2005(
                        span,
                        format!(
                            "cannot close non-channel `{}` (type `{}`) (HYB-E2005)",
                            op.render(),
                            tp
                        ),
                        "`close` requires a bidirectional or send-only channel (`chan T` or `chan<- T`)",
                    );
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
                if let Some(tp) = self.expr_type_param(map_arg) {
                    self.emit_e2005(
                        span,
                        format!(
                            "invalid argument `{}` (type `{}`) is not a map in `delete` (HYB-E2005)",
                            map_arg.render(),
                            tp
                        ),
                        "`delete(m, k)` requires a map as its first argument",
                    );
                    return;
                }
                let mk = self.kind_of_expr(map_arg);
                match &mk {
                    // #2474: `delete(s, v)` on a `set[T]` (lowered to
                    // `s.remove(v)`) needs `v` of type `T`.
                    OpKind::Unknown => {
                        if let Some(elem) = self.set_elem_of_expr(map_arg) {
                            let want = self.kind_of_named(&elem, 0);
                            // #3518: reject untyped nil passed to non-nilable set element types
                            if Checker::is_nil_literal(key_arg) && is_non_nilable(&want) {
                                self.emit_e2005(
                                    span,
                                    format!(
                                        "cannot use `nil` as element type `{}` in `delete` on `set[{}]` (HYB-E2005)",
                                        want.label(),
                                        elem
                                    ),
                                    "the second argument to `delete(s, v)` must match the set's element type",
                                );
                                return;
                            }
                            let got = self.kind_of_expr(key_arg);
                            let int_to_float_conflict = want == OpKind::Float
                                && got == OpKind::Int
                                && !self.is_untyped_int_const(key_arg);
                            // #3240: compare scalar width and sized array dimensions for delete on set[T]
                            let del_elem_scalar = canonical_numeric_name(&elem);
                            let del_arg_scalar = self.typed_value_scalar(key_arg);
                            let scalar_mismatch = matches!(
                                (del_elem_scalar, del_arg_scalar),
                                (Some(a), Some(b)) if a != b
                            );
                            let arg_ty = self.typed_value_type_name(key_arg);
                            let array_mismatch = self.sized_array_names_differ(
                                Some(&elem),
                                arg_ty.as_deref(),
                            );
                            if kinds_conflict(&want, &got)
                                || int_to_float_conflict
                                || scalar_mismatch
                                || array_mismatch
                            {
                                let got_label = del_arg_scalar
                                    .map(|s| s.to_string())
                                    .or_else(|| arg_ty.clone())
                                    .unwrap_or_else(|| got.label());
                                self.emit_e2005(
                                    span,
                                    format!(
                                        "cannot use `{}` (type `{}`) as element type `{}` in `delete` on `set[{}]` (HYB-E2005)",
                                        key_arg.render(),
                                        got_label,
                                        elem,
                                        elem
                                    ),
                                    "the second argument to `delete(s, v)` must match the set's element type",
                                );
                            }
                        }
                    }
                    OpKind::Map(k_expected, _) => {
                        let kk = self.kind_of_expr(key_arg);
                        let is_untyped_int_to_float = **k_expected == OpKind::Float
                            && kk == OpKind::Int
                            && self.is_untyped_int_const(key_arg);
                        // #3292: same width blindness as the index arm --
                        // `map[int32]V` with an `int64` key both look like
                        // `OpKind::Int`. Compare the declared spellings too.
                        let del_key_scalar = self.map_key_scalar_name(Some(map_arg));
                        let del_arg_scalar = self.typed_value_scalar(key_arg);
                        let del_width_conflict = matches!(
                            (del_key_scalar, del_arg_scalar),
                            (Some(a), Some(b)) if a != b
                        );
                        // #3240: compare sized array length on map key arguments
                        let map_key_ty = self.map_key_type_name(Some(map_arg));
                        let arg_ty = self.typed_value_type_name(key_arg);
                        let del_array_conflict = self.sized_array_names_differ(
                            map_key_ty.as_deref(),
                            arg_ty.as_deref(),
                        );
                        // #3518: reject untyped nil passed to non-nilable map key types
                        if Checker::is_nil_literal(key_arg) && is_non_nilable(k_expected) {
                            let k_label = del_key_scalar
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| k_expected.label());
                            self.emit_e2005(
                                span,
                                format!(
                                    "cannot use `nil` as key type `{}` in `delete` (HYB-E2005)",
                                    k_label
                                ),
                                "the second argument to `delete(m, k)` must match the map's key type",
                            );
                            return;
                        }
                        if (**k_expected != OpKind::Unknown
                            && kk != OpKind::Unknown
                            && kk != **k_expected
                            && !is_untyped_int_to_float)
                            || del_width_conflict
                            || del_array_conflict
                        {
                            let got_label = del_arg_scalar
                                .map(|s| s.to_string())
                                .or_else(|| arg_ty.clone())
                                .unwrap_or_else(|| kk.label());
                            let want_label = del_key_scalar
                                .map(|s| s.to_string())
                                .or_else(|| map_key_ty.clone())
                                .unwrap_or_else(|| k_expected.label());
                            self.emit_e2005(
                                span,
                                format!(
                                    "cannot use `{}` (type `{}`) as key type `{}` in `delete` (HYB-E2005)",
                                    key_arg.render(),
                                    got_label,
                                    want_label
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
                if let Some(tp) = self.expr_type_param(slice_arg) {
                    self.emit_e2005(
                        span,
                        format!(
                            "invalid `append`: first argument `{}` (type `{}`) must be a slice (HYB-E2005)",
                            slice_arg.render(),
                            tp
                        ),
                        "the first argument to `append(xs, ...)` must be a slice (`[]T`)",
                    );
                    return;
                }
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
            "copy" => {
                if args.len() != 2 {
                    return; // arity handled by builtins.rs
                }
                let dst_arg = &args[0].value;
                let src_arg = &args[1].value;
                if let Some(tp) = self.expr_type_param(dst_arg) {
                    self.emit_e2005(
                        span,
                        format!(
                            "first argument to `copy` must be a slice, found `{}` (type `{}`) (HYB-E2005)",
                            dst_arg.render(),
                            tp
                        ),
                        "the first argument to `copy(dst, src)` must be a slice (`[]T`)",
                    );
                    return;
                }
                if let Some(tp) = self.expr_type_param(src_arg) {
                    self.emit_e2005(
                        span,
                        format!(
                            "second argument to `copy` must be a slice or string, found `{}` (type `{}`) (HYB-E2005)",
                            src_arg.render(),
                            tp
                        ),
                        "the second argument to `copy(dst, src)` must be a slice (`[]T`) or string",
                    );
                    return;
                }
                let dk = self.kind_of_expr(dst_arg);
                let sk = self.kind_of_expr(src_arg);

                if !matches!(dk, OpKind::Unknown | OpKind::Slice(_)) {
                    self.emit_e2005(
                        span,
                        format!(
                            "first argument to `copy` must be a slice, found `{}` (type `{}`) (HYB-E2005)",
                            dst_arg.render(),
                            dk.label()
                        ),
                        "the first argument to `copy(dst, src)` must be a slice (`[]T`)",
                    );
                    return;
                }

                if !matches!(sk, OpKind::Unknown | OpKind::Slice(_) | OpKind::Str) {
                    self.emit_e2005(
                        span,
                        format!(
                            "second argument to `copy` must be a slice or string, found `{}` (type `{}`) (HYB-E2005)",
                            src_arg.render(),
                            sk.label()
                        ),
                        "the second argument to `copy(dst, src)` must be a slice (`[]T`) or string",
                    );
                    return;
                }

                match (&dk, &sk) {
                    (OpKind::Slice(dst_elem), OpKind::Slice(src_elem)) => {
                        let dst_elem_ty = self.slice_element_type_name(dst_arg);
                        let src_elem_ty = self.slice_element_type_name(src_arg);
                        let elem_ty_conflict = match (&dst_elem_ty, &src_elem_ty) {
                            (Some(d), Some(s)) => {
                                let cd = canonical_numeric_name(d).unwrap_or(d.as_str());
                                let cs = canonical_numeric_name(s).unwrap_or(s.as_str());
                                cd != cs
                            }
                            _ => false,
                        };
                        if copy_element_kinds_conflict(dst_elem, src_elem) || elem_ty_conflict {
                            let d_label = dst_elem_ty.unwrap_or_else(|| dst_elem.label().to_string());
                            let s_label = src_elem_ty.unwrap_or_else(|| src_elem.label().to_string());
                            self.emit_e1002(
                                span,
                                format!(
                                    "arguments to `copy` have different element types: `{}` and `{}` (HYB-E1002)",
                                    d_label,
                                    s_label
                                ),
                                "arguments to `copy` must have identical element types",
                            );
                        }
                    }
                    (OpKind::Slice(dst_elem), OpKind::Str) => {
                        let dst_elem_ty = self.slice_element_type_name(dst_arg);
                        let is_byte = dst_elem_ty.as_deref().map_or(false, |t| {
                            matches!(crate::const_overflow::canonical_int_type_name(t), "uint8")
                        });
                        let is_unknown = matches!(**dst_elem, OpKind::Unknown) && dst_elem_ty.is_none();
                        if !is_byte && !is_unknown {
                            let label = dst_elem_ty.unwrap_or_else(|| dst_elem.label().to_string());
                            self.emit_e1002(
                                span,
                                format!(
                                    "cannot copy string into slice with element type `{}` (HYB-E1002)",
                                    label
                                ),
                                "copying from a string requires a byte slice destination (`[]byte`)",
                            );
                        }
                    }
                    _ => {}
                }
            }
            "clear" => {
                if args.len() != 1 {
                    return; // arity handled by builtins.rs
                }
                let op = &args[0].value;
                if let Some(tp) = self.expr_type_param(op) {
                    self.emit_e2005(
                        span,
                        format!(
                            "cannot clear non-slice/map `{}` (type `{}`) (HYB-E2005)",
                            op.render(),
                            tp
                        ),
                        "`clear` requires a slice or map operand",
                    );
                    return;
                }
                let ak = self.kind_of_expr(op);
                if !matches!(ak, OpKind::Unknown | OpKind::Slice(_) | OpKind::Map(_, _)) {
                    self.emit_e2005(
                        span,
                        format!(
                            "cannot clear non-slice/map `{}` (type `{}`) (HYB-E2005)",
                            op.render(),
                            ak.label()
                        ),
                        "`clear` requires a slice or map operand",
                    );
                }
            }

            "real" | "imag" => {
                if args.len() != 1 {
                    return; // arity handled by builtins.rs
                }
                let op = &args[0].value;
                let ak = self.kind_of_expr(op);
                if matches!(
                    ak,
                    OpKind::Str
                        | OpKind::Bool
                        | OpKind::Slice(_)
                        | OpKind::Array(_)
                        | OpKind::Map(_, _)
                        | OpKind::Chan(_)
                        | OpKind::Struct(_)
                        | OpKind::Pointer(_)
                        | OpKind::Fn(..)
                        | OpKind::Interface
                ) {
                    self.emit_e1002(
                        op.span(),
                        format!(
                            "invalid argument `{}` (type `{}`) for built-in `{name}` — Go refuses `invalid argument: {} (must be complex)` (HYB-E1002)",
                            op.render(),
                            ak.label(),
                            op.render()
                        ),
                        "argument to `real` or `imag` must be a complex number (`complex64` or `complex128`)",
                    );
                }
            }
            "complex" => {
                if args.len() != 2 {
                    return; // arity handled by builtins.rs
                }
                for arg in args {
                    let ak = self.kind_of_expr(&arg.value);
                    if matches!(
                        ak,
                        OpKind::Str
                            | OpKind::Bool
                            | OpKind::Slice(_)
                            | OpKind::Array(_)
                            | OpKind::Map(_, _)
                            | OpKind::Chan(_)
                            | OpKind::Struct(_)
                            | OpKind::Pointer(_)
                            | OpKind::Fn(..)
                            | OpKind::Interface
                    ) {
                        self.emit_e1002(
                            arg.value.span(),
                            format!(
                                "invalid argument `{}` (type `{}`) in `complex` — Go refuses `invalid argument: {} (must be float)` (HYB-E1002)",
                                arg.value.render(),
                                ak.label(),
                                arg.value.render()
                            ),
                            "arguments to `complex` must be floating-point numbers (`float32` or `float64`)",
                        );
                    }
                }
            }
            "new" => {
                if args.len() != 1 {
                    return; // arity handled by builtins.rs
                }
                let op = &args[0].value;
                self.check_new_type_operand(op, span);
            }
            "min" | "max" => {
                let mut first_arg: Option<(&Expr, OpKind)> = None;
                let mut first_typed_scalar: Option<(&Expr, &'static str)> = None;
                for arg in args {
                    if let Some(tp) = self.expr_type_param(&arg.value) {
                        self.emit_e2005(
                            span,
                            format!(
                                "invalid argument `{}` (type `{}`) for `{name}`: type has no order (HYB-E2005)",
                                arg.value.render(),
                                tp
                            ),
                            "the arguments to `min` and `max` must have ordered type (integers, floats, or strings)",
                        );
                        return;
                    }
                    let k = self.kind_of_expr(&arg.value);
                    if matches!(k, OpKind::Unknown | OpKind::Void) {
                        continue;
                    }
                    let is_ordered = matches!(k, OpKind::Int | OpKind::Float | OpKind::Str);
                    if !is_ordered {
                        self.emit_e2005(
                            span,
                            format!(
                                "invalid argument `{}` (type `{}`) for `{name}`: type has no order (HYB-E2005)",
                                arg.value.render(),
                                k.label()
                            ),
                            "the arguments to `min` and `max` must have ordered type (integers, floats, or strings)",
                        );
                        return;
                    }
                    if let Some((prev_arg, prev_k)) = &first_arg {
                        let is_compatible = if *prev_k == k {
                            true
                        } else if *prev_k == OpKind::Float && k == OpKind::Int {
                            self.is_untyped_int_const(&arg.value)
                        } else if *prev_k == OpKind::Int && k == OpKind::Float {
                            self.is_untyped_int_const(prev_arg)
                        } else {
                            false
                        };
                        if !is_compatible {
                            self.emit_e1002(
                                span,
                                format!(
                                    "mismatched types `{}` and `{}` in `{name}` (HYB-E1002)",
                                    prev_k.label(),
                                    k.label()
                                ),
                                "all arguments to `min` and `max` must have matching ordered types",
                            );
                            return;
                        }
                    } else {
                        first_arg = Some((&arg.value, k));
                    }

                    if let Some(s) = self.typed_value_scalar(&arg.value) {
                        if let Some((_prev_arg, prev_s)) = first_typed_scalar {
                            if prev_s != s {
                                self.emit_e1002(
                                    span,
                                    format!(
                                        "mismatched types `{}` and `{}` in `{name}` (HYB-E1002)",
                                        prev_s,
                                        s
                                    ),
                                    "all arguments to `min` and `max` must have matching ordered types",
                                );
                                return;
                            }
                        } else {
                            first_typed_scalar = Some((&arg.value, s));
                        }
                    }
                }
            }

            _ => {}
        }
    }

    fn check_new_type_operand(&mut self, op: &Expr, span: Span) {
        let bare = unwrap_parens(op);
        if matches!(bare, Expr::Lit { .. })
            || matches!(
                bare,
                Expr::Unary {
                    op: UnOp::Neg,
                    operand,
                    ..
                } if matches!(unwrap_parens(operand), Expr::Lit { .. })
            )
        {
            return; // already handled by builtins.rs
        }
        match bare {
            Expr::Ident { name, .. } => {
                if let Some(k) = self.lookup_kind(name) {
                    self.emit_e2005(
                        span,
                        format!(
                            "cannot use `{name}` (variable of type `{}`) as type in argument to `new` (HYB-E2005)",
                            k.label()
                        ),
                        "`new` takes a type name — not a variable",
                    );
                } else if self.file_consts.contains_key(name) {
                    self.emit_e2005(
                        span,
                        format!("cannot use `{name}` (constant) as type in argument to `new` (HYB-E2005)"),
                        "`new` takes a type name — not a constant",
                    );
                } else if self.ck.fns.contains_key(name) {
                    self.emit_e2005(
                        span,
                        format!("cannot use `{name}` (function) as type in argument to `new` (HYB-E2005)"),
                        "`new` takes a type name — not a function",
                    );
                } else if name == "true" || name == "false" || name == "nil" {
                    self.emit_e2005(
                        span,
                        format!("cannot use `{name}` as type in argument to `new` (HYB-E2005)"),
                        "`new` takes a type name — not a literal",
                    );
                }
            }
            Expr::SliceLit {
                elem: Some(_),
                items,
                ..
            } if items.is_empty() => {}
            Expr::MapLit { entries, .. } if entries.is_empty() => {}
            Expr::Index { base, .. }
                if matches!(unwrap_parens(base), Expr::Ident { name, .. } if self.lookup_kind(name).is_none() && !self.file_consts.contains_key(name) && !self.ck.fns.contains_key(name)) => {}
            Expr::StructLit {
                ty: Some(TypeExpr::Chan { .. }),
                fields,
                ..
            } if fields.is_empty() => {}
            Expr::Field { base, .. } => {
                if let Expr::Ident { name: bname, .. } = unwrap_parens(base) {
                    if self.lookup_kind(bname).is_some() {
                        self.emit_e2005(
                            span,
                            format!("cannot use `{}` as type in argument to `new` (HYB-E2005)", op.render()),
                            "`new` takes a type — not a field access on a variable",
                        );
                    }
                } else {
                    self.emit_e2005(
                        span,
                        format!("cannot use `{}` as type in argument to `new` (HYB-E2005)", op.render()),
                        "`new` takes a type name — not an expression",
                    );
                }
            }
            _ => {
                self.emit_e2005(
                    span,
                    format!("argument to `new` must be a type, found expression `{}` (HYB-E2005)", op.render()),
                    "`new` takes a type name — not a value or expression",
                );
            }
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

    fn check_io_call_buildable(&mut self, name: &str, args: &[CallArg], span: Span) {
        match name {
            "eof"
            | "err_unexpected_eof"
            | "err_no_progress"
            | "err_short_buffer"
            | "err_short_write"
            | "err_closed_pipe"
            | "discard"
            | "reader"
            | "writer"
            | "closer"
            | "seeker"
            | "read_closer"
            | "write_closer"
            | "read_writer"
            | "pipe_reader"
            | "pipe_writer"
            | "section_reader"
            | "limited_reader"
            | "offset_writer" => {
                self.emit_e1002(
                    span,
                    format!("cannot call `io.{}`: package symbol is a variable or type, not a function (HYB-E1002)", name),
                    &format!("refer to `io.{}` as a value or type rather than calling it", name),
                );
            }
            "pipe" => {
                if !args.is_empty() {
                    self.emit_e1002(
                        span,
                        format!("`io.pipe` takes 0 arguments `()`, got {} (HYB-E1002)", args.len()),
                        "write `io.pipe()`",
                    );
                }
            }
            "read_all" => {
                if args.len() != 1 {
                    self.emit_e1002(
                        span,
                        format!("`io.read_all` takes exactly 1 argument `(r)`, got {} (HYB-E1002)", args.len()),
                        "write `io.read_all(r)`",
                    );
                }
            }
            "copy" => {
                if args.len() != 2 {
                    self.emit_e1002(
                        span,
                        format!("`io.copy` takes exactly 2 arguments `(dst, src)`, got {} (HYB-E1002)", args.len()),
                        "write `io.copy(dst, src)`",
                    );
                }
            }
            "read_full" => {
                if args.len() != 2 {
                    self.emit_e1002(
                        span,
                        format!("`io.read_full` takes exactly 2 arguments `(r, buf)`, got {} (HYB-E1002)", args.len()),
                        "write `io.read_full(r, buf)`",
                    );
                }
            }
            "write_string" => {
                if args.len() != 2 {
                    self.emit_e1002(
                        span,
                        format!("`io.write_string` takes exactly 2 arguments `(w, s)`, got {} (HYB-E1002)", args.len()),
                        "write `io.write_string(w, s)`",
                    );
                }
            }
            "limit_reader" => {
                if args.len() != 2 {
                    self.emit_e1002(
                        span,
                        format!("`io.limit_reader` takes exactly 2 arguments `(r, n)`, got {} (HYB-E1002)", args.len()),
                        "write `io.limit_reader(r, n)`",
                    );
                }
            }
            "tee_reader" => {
                if args.len() != 2 {
                    self.emit_e1002(
                        span,
                        format!("`io.tee_reader` takes exactly 2 arguments `(r, w)`, got {} (HYB-E1002)", args.len()),
                        "write `io.tee_reader(r, w)`",
                    );
                }
            }
            "copy_n" => {
                if args.len() != 3 {
                    self.emit_e1002(
                        span,
                        format!("`io.copy_n` takes exactly 3 arguments `(dst, src, n)`, got {} (HYB-E1002)", args.len()),
                        "write `io.copy_n(dst, src, n)`",
                    );
                }
            }
            "copy_buffer" => {
                if args.len() != 3 {
                    self.emit_e1002(
                        span,
                        format!("`io.copy_buffer` takes exactly 3 arguments `(dst, src, buf)`, got {} (HYB-E1002)", args.len()),
                        "write `io.copy_buffer(dst, src, buf)`",
                    );
                }
            }
            "read_at_least" => {
                if args.len() != 3 {
                    self.emit_e1002(
                        span,
                        format!("`io.read_at_least` takes exactly 3 arguments `(r, buf, min)`, got {} (HYB-E1002)", args.len()),
                        "write `io.read_at_least(r, buf, min)`",
                    );
                }
            }
            "new_section_reader" => {
                if args.len() != 3 {
                    self.emit_e1002(
                        span,
                        format!("`io.new_section_reader` takes exactly 3 arguments `(r, off, n)`, got {} (HYB-E1002)", args.len()),
                        "write `io.new_section_reader(r, off, n)`",
                    );
                }
            }
            "multi_reader" => {
                if args.is_empty() {
                    self.emit_e1002(
                        span,
                        "`io.multi_reader` requires at least 1 reader argument `(readers...)`, got 0 (HYB-E1002)".to_string(),
                        "write `io.multi_reader(r1, ...)`",
                    );
                }
            }
            "multi_writer" => {
                if args.is_empty() {
                    self.emit_e1002(
                        span,
                        "`io.multi_writer` requires at least 1 writer argument `(writers...)`, got 0 (HYB-E1002)".to_string(),
                        "write `io.multi_writer(w1, ...)`",
                    );
                }
            }
            _ => {}
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

    #[test]
    fn rejects_composite_array_out_of_bounds_index_and_slice_3694() {
        for src in [
            "import \"fmt\"\nfn main() {\n    var a [2][3]int\n    fmt.println(a[1][3])\n}\n",
            "import \"fmt\"\nfn main() {\n    var a [2][3][4]int\n    fmt.println(a[1][2][4])\n}\n",
            "fn main() {\n    var a [2][3]int\n    a[1][3] = 1\n    _ = a\n}\n",
            "fn main() {\n    var a [2][3]int\n    a[1][3] += 1\n    _ = a\n}\n",
            "import \"fmt\"\nstruct s_t { a [3]int }\nfn main() {\n    var s s_t\n    fmt.println(s.a[3])\n}\n",
            "import \"fmt\"\nstruct s_t { a [3]int }\nfn main() {\n    var s [2]s_t\n    fmt.println(s[0].a[3])\n}\n",
            "import \"fmt\"\nfn f() [3]int {\n    return [3]int{1, 2, 3}\n}\nfn main() {\n    fmt.println(f()[3])\n}\n",
            "import \"fmt\"\nfn main() {\n    var s [][3]int = make([][3]int, 2)\n    fmt.println(s[0][3])\n}\n",
            "import \"fmt\"\nfn main() {\n    var a [2][3]int\n    p := &a\n    fmt.println(p[1][3])\n}\n",
            "import \"fmt\"\nfn main() {\n    var a [2][3]int\n    fmt.println(a[1][1:5])\n}\n",
            "import \"fmt\"\nfn main() {\n    var m map[string][3]int\n    fmt.println(m[\"x\"][3])\n}\n",
        ] {
            let r = check(src);
            assert_eq!(r.verdict, Verdict::Reject, "expected reject for:\n{src}");
            assert!(
                error_codes(&r).contains(&"HYB-E1002".to_string()),
                "expected HYB-E1002, got {:?} for:\n{src}",
                error_codes(&r)
            );
        }

        // Valid in-bounds controls must pass cleanly.
        let ok = "\
import \"fmt\"
struct s_t { a [3]int }
fn f() [3]int { return [3]int{1, 2, 3} }
fn main() {
    var a [2][3]int
    a[1][2] = 9
    p := &a
    var s s_t
    var sa [2]s_t
    var sl [][3]int = make([][3]int, 2)
    var m map[string][3]int = map[string][3]int{\"x\": [3]int{1, 2, 3}}
    fmt.println(a[1][2], p[1][2], a[1][1:3], s.a[2], sa[0].a[2], f()[2], sl[0][2], m[\"x\"][2])
}
";
        let r = check(ok);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "unexpected diagnostics: {:?}",
            r.diagnostics
        );
    }
}

/// The element type hy-emit's `infer_slice_head` gives an untyped slice
/// literal from its first item, or `None` when it infers none.
fn untyped_slice_head_ty(first: &Expr) -> Option<TypeExpr> {
    let span = first.span();
    let named = |n: &str| Some(TypeExpr::Named { name: n.to_string(), span });
    match first {
        Expr::Paren { inner, .. } => untyped_slice_head_ty(inner),
        Expr::Unary {
            op: UnOp::Neg,
            operand,
            ..
        } => untyped_slice_head_ty(operand).filter(|t| {
            matches!(t, TypeExpr::Named { name, .. } if name == "int" || name == "float64")
        }),
        Expr::Lit { kind, .. } => match kind {
            LitKind::Int(_) => named("int"),
            LitKind::Float(_) => named("float64"),
            LitKind::Str(_) | LitKind::RawStr(_) => named("string"),
            LitKind::Bool(_) => named("bool"),
            LitKind::Char(_) => named("rune"),
            LitKind::Nil => None,
        },
        Expr::StructLit { ty: Some(t), .. } => Some(t.clone()),
        _ => None,
    }
}


fn block_has_try(b: &Block) -> bool {
    b.stmts.iter().any(stmt_has_try)
}

fn stmt_has_try(s: &Stmt) -> bool {
    match s {
        Stmt::Local(l) => l.values.iter().any(expr_has_try),
        Stmt::Assign { targets, values, .. } => {
            targets.iter().any(place_has_try) || values.iter().any(expr_has_try)
        }
        Stmt::Expr { expr, .. } => expr_has_try(expr),
        Stmt::Send { chan, value, .. } => expr_has_try(chan) || expr_has_try(value),
        Stmt::If { init, cond, then, otherwise, .. } => {
            init.as_deref().map_or(false, stmt_has_try)
                || expr_has_try(cond)
                || block_has_try(then)
                || otherwise.as_deref().map_or(false, stmt_has_try)
        }
        Stmt::For { kind, body, .. } => {
            let kind_try = match kind {
                ForKind::While { cond } => expr_has_try(cond),
                ForKind::Range { lo, hi, .. } => expr_has_try(lo) || expr_has_try(hi),
                ForKind::Iterate { expr, .. } => expr_has_try(expr),
                ForKind::Infinite => false,
            };
            kind_try || block_has_try(body)
        }
        Stmt::Return { values, .. } => values.iter().any(expr_has_try),
        Stmt::Task { callee, args, .. } => {
            expr_has_try(callee) || args.iter().any(|a| expr_has_try(&a.value))
        }
        Stmt::Managed { body, .. } => block_has_try(body),
        Stmt::Match { subject, arms, .. } => {
            expr_has_try(subject)
                || arms.iter().any(|a| {
                    a.guard.as_ref().map_or(false, expr_has_try) || block_has_try(&a.body)
                })
        }
        Stmt::Defer { call, .. } => expr_has_try(call),
        Stmt::Block { block, .. } => block_has_try(block),
        Stmt::Select { arms, .. } => arms.iter().any(|a| {
            let arm_try = match &a.kind {
                SelectArmKind::Send { chan, value, .. } => {
                    expr_has_try(chan) || expr_has_try(value)
                }
                SelectArmKind::Recv { chan, .. } => expr_has_try(chan),
                SelectArmKind::Default => false,
            };
            arm_try || block_has_try(&a.body)
        }),
        Stmt::Break { .. } | Stmt::Continue { .. } => false,
    }
}

fn place_has_try(p: &Place) -> bool {
    match &p.kind {
        PlaceKind::Ident(_) => false,
        PlaceKind::Index { base, index } => place_has_try(base) || expr_has_try(index),
        PlaceKind::Field { base, .. } => place_has_try(base),
        PlaceKind::AddrOf { base } => place_has_try(base),
    }
}

fn expr_has_try(e: &Expr) -> bool {
    match unwrap_parens(e) {
        Expr::Try { .. } => true,
        Expr::Unary { operand, .. } => expr_has_try(operand),
        Expr::Binary { lhs, rhs, .. } => expr_has_try(lhs) || expr_has_try(rhs),
        Expr::Call { callee, args, .. } => {
            expr_has_try(callee) || args.iter().any(|a| expr_has_try(&a.value))
        }
        Expr::Field { base, .. } => expr_has_try(base),
        Expr::Index { base, index, .. } => expr_has_try(base) || expr_has_try(index),
        Expr::Slice { base, lo, hi, .. } => {
            expr_has_try(base)
                || lo.as_deref().map_or(false, expr_has_try)
                || hi.as_deref().map_or(false, expr_has_try)
        }
        Expr::Interp { parts, .. } => parts.iter().any(|p| match p {
            InterpPart::Expr(e) => expr_has_try(e),
            _ => false,
        }),
        Expr::Move { operand, .. } | Expr::AddrOf { operand, .. } => expr_has_try(operand),
        Expr::Receive { chan, .. } => expr_has_try(chan),
        Expr::SliceLit { items, .. } => items.iter().any(expr_has_try),
        Expr::MapLit { entries, .. } => {
            entries.iter().any(|(k, v)| expr_has_try(k) || expr_has_try(v))
        }
        Expr::StructLit { fields, .. } => fields.iter().any(|f| expr_has_try(&f.value)),
        Expr::Paren { inner, .. } => expr_has_try(inner),
        Expr::Ident { .. } | Expr::Lit { .. } | Expr::FnLit { .. } => false,
    }
}
