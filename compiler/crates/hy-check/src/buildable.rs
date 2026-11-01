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
    Fn(Vec<OpKind>, Option<usize>),
    /// #2823 — `option[T]` carries its payload kind. `Unknown` here would be
    /// fail-open in every operand guard, so `op + 1`, `takes_int(op)` and
    /// `var s string = op.unwrap()` all passed and failed `go build`.
    Option(Box<OpKind>),
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
            OpKind::Option(e) => format!("option[{}]", e.label()),
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
        // #2823: an option only satisfies an option (with a compatible
        // payload); every other pairing falls through to the mismatch arm, so
        // `op + 1` and `takes_int(op)` are refused exactly as Go refuses them.
        (Option(a), Option(b)) => kinds_conflict(a, b),
        // Fn kinds only carry results here, so a result match is not proof of
        // a signature match; stay permissive.
        (Fn(..), Fn(..)) => false,
        // Everything else is a shape or scalar-family mismatch Go refuses.
        _ => true,
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
fn is_non_nilable(k: &OpKind) -> bool {
    matches!(
        k,
        OpKind::Int
            | OpKind::Float
            | OpKind::Bool
            | OpKind::Str
            | OpKind::Struct(_)
            | OpKind::Array(_)
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
        (Pointer(_), Interface) => false,
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
    /// #1031 — exact predeclared numeric type of each result of the enclosing
    /// signature (`None` = not a predeclared numeric spelling). Swapped
    /// together with `fn_results`.
    fn_result_scalars: Vec<Option<&'static str>>,
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
            fn_type_params: HashSet::new(),
            fn_results_all_named: false,
            fn_results_error_tail: false,
            scalar_scopes: Vec::new(),
            fn_result_scalars: Vec::new(),
            loop_depth: 0,
            select_depth: 0,
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
        self.set_elem_scopes.push(HashMap::new());
        self.copy_var_scopes.push(HashSet::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
        self.kind_scopes.pop();
        self.type_name_scopes.pop();
        self.scalar_scopes.pop();
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
                }
                None
            }
            Expr::SliceLit { elem: Some(t), .. } => Some(t.render()),
            Expr::Slice { base, .. } => self.slice_element_type_name(base),
            Expr::Call { callee, args, .. } => {
                if let Expr::Ident { name, .. } = unwrap_parens(callee) {
                    if name == "make" {
                        if let Some(first) = args.first() {
                            return self.slice_element_type_name(&first.value);
                        }
                    }
                }
                None
            }
            _ => None,
        }
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

    /// #2770 — the canonical numeric spelling of a simple local target, for the
    /// assignment/argument scalar comparison. Non-ident places fail open.
    fn place_scalar(&self, p: &Place) -> Option<&'static str> {
        match &p.kind {
            PlaceKind::Ident(n) => self.lookup_scalar(n),
            _ => None,
        }
    }

    /// #2771 — the declared Go spelling of a value whose type carries detail
    /// that `OpKind` drops, notably an array's length (`[3]int` vs `[4]int`).
    /// Only identifiers bound with an explicit type name are known; everything
    /// else (literals, conversions, calls) fails open.
    fn typed_value_type_name(&self, e: &Expr) -> Option<String> {
        match unwrap_parens(e) {
            Expr::Ident { name, .. } => self.lookup_type_name(name),
            _ => None,
        }
    }

    /// #2771 — the declared Go spelling of a simple local assignment target.
    fn place_type_name(&self, p: &Place) -> Option<String> {
        match &p.kind {
            PlaceKind::Ident(n) => self.lookup_type_name(n),
            _ => None,
        }
    }

    /// #2771 — two KNOWN and DIFFERENT sized-array spellings. Slices, named
    /// types and unparsed lengths fail open so aliases never false-reject.
    fn sized_array_names_differ(&self, a: Option<&str>, b: Option<&str>) -> bool {
        matches!((a, b), (Some(x), Some(y))
            if x != y && is_sized_array_spelling(x) && is_sized_array_spelling(y))
    }
    /// #1031 — the exact predeclared numeric spelling of a type expression.
    /// A user-declared struct/enum/alias of that name is not the predeclared
    /// type, so it fails open.
    fn scalar_of_ty(&self, ty: &TypeExpr) -> Option<&'static str> {
        let TypeExpr::Named { name, .. } = ty else {
            return None;
        };
        if self.ck.structs.contains_key(name.as_str())
            || self.ck.enums.contains_key(name.as_str())
            || self.ck.type_aliases.contains_key(name.as_str())
        {
            return None;
        }
        canonical_numeric_name(name)
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
            if f.name == "main" && (!f.params.is_empty() || !f.type_params.is_empty()) {
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
        let outer_scalars = std::mem::replace(&mut self.fn_result_scalars, result_scalars);
        let outer = std::mem::replace(&mut self.fn_results, result_kinds);
        let outer_named = std::mem::replace(&mut self.fn_results_all_named, all_named);
        let outer_err = std::mem::replace(&mut self.fn_results_error_tail, error_tail);
        let outer_tp = std::mem::replace(
            &mut self.fn_type_params,
            f.type_params.iter().map(|p| p.name.clone()).collect(),
        );
        let outer_loop = std::mem::replace(&mut self.loop_depth, 0);
        let outer_select = std::mem::replace(&mut self.select_depth, 0);
        self.block_stmts(body);
        self.loop_depth = outer_loop;
        self.select_depth = outer_select;
        self.fn_results = outer;
        self.fn_results_all_named = outer_named;
        self.fn_results_error_tail = outer_err;
        self.fn_result_scalars = outer_scalars;
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
        if l.names.len() == 1 && l.values.len() == 1 {
            self.check_single_value_position(&l.values[0], "a binding", /*binding_position=*/ true);
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
                if let Some(elem) = self.slice_element_type_name(val) {
                    self.bind_type_name(name, format!("[]{}", elem));
                } else if let Expr::Call { callee, .. } = unwrap_parens(val) {
                    // #2884: `src := mk64()` — the bound name has no declared
                    // type text, and the call's kind collapses the width, so
                    // record the callee's single SLICE result type. That is what
                    // lets a later `append(dst_int32, src...)` see `[]int64`.
                    if let Expr::Ident { name: callee_name, .. } = unwrap_parens(callee) {
                        let head = Checker::base_named_type_str(callee_name).to_string();
                        if let Some(f) = self
                            .ck
                            .fns
                            .get(callee_name.as_str())
                            .or_else(|| self.ck.fns.get(&head))
                        {
                            if f.type_params.is_empty() && f.results.len() == 1 {
                                let text = f.results[0].ty.render();
                                if text.starts_with("[]") {
                                    self.bind_type_name(name, text);
                                }
                            }
                        }
                    }
                }
            }
        }
        for (((name, kind), scalar), set_elem) in
            l.names.iter().zip(kinds).zip(scalars).zip(set_elems)
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
                if targets.len() == 1 && values.len() == 1 {
                    self.check_single_value_position(&values[0], "an assignment", /*binding_position=*/ true);
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
                    _ => {}
                }
            }
            Stmt::Managed { body, .. } => self.block_scoped(body),
            Stmt::Match { subject, arms, .. } => {
                self.expr(subject);
                let subject_kind = self.kind_of_expr(subject);
                for arm in arms {
                    self.push_scope();
                    match &arm.pattern {
                        MatchPattern::Path { segments, binding } => {
                            if let Some(root) = segments.first() {
                                self.mention(root);
                            }
                            if let Some(b) = binding {
                                self.define_elidable(b, subject.span());
                                self.bind_kind(b, OpKind::Unknown);
                            }
                        }
                        MatchPattern::Lit(lit) => {
                            self.check_match_lit_pattern(&subject_kind, lit, subject.span());
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
            PlaceKind::Field { .. } | PlaceKind::AddrOf { .. } => {
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
            Expr::AddrOf { operand, span, .. } => {
                self.expr(operand);
                self.check_addr_of_operand(operand, *span);
            }
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
                        if let Expr::Ident { name, span } = unwrap_parens(&first_arg.value) {
                            self.check_named_type_instantiation(name, *span);
                        }
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
            TypeExpr::Option { elem, .. } => {
                OpKind::Option(Box::new(self.kind_of_ty_with_subst(elem, subst, depth + 1)))
            }
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
                                Some(m.params.len()),
                            );
                        }
                    }
                }
                // A method expression (`g := pt.get`) names the method through
                // its TYPE, so the receiver becomes the first parameter: the
                // function value takes `params + 1` arguments.
                if let Some(m) = self.method_expr_decl(base, name) {
                    return OpKind::Fn(
                        m.results
                            .iter()
                            .map(|r| self.kind_of_ty(&r.ty, 0))
                            .collect(),
                        Some(m.params.len() + 1),
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
                    "len" | "cap" | "copy" => return Some(vec![OpKind::Int]),
                    "append" => {
                        let first = args
                            .first()
                            .map(|a| self.kind_of_expr(&a.value))
                            .unwrap_or_else(|| OpKind::Slice(Box::new(OpKind::Unknown)));
                        return Some(vec![first]);
                    }
                    "make" => return Some(vec![self.make_kind(args)]),
                    "new" => return Some(vec![OpKind::Pointer(Box::new(OpKind::Unknown))]),
                    "delete" | "close" | "clear" | "panic" => return Some(Vec::new()),
                    "recover" => return Some(vec![OpKind::Interface]),
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
                    let conflict = (want == OpKind::Float && got == OpKind::Int && !is_untyped_int_to_float)
                        || kinds_conflict(&want, &got)
                        || scalar_mismatch;
                    if conflict {
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
        // parameter list does not describe; leave them to the type checker.
        if !f.type_params.is_empty() {
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
                    let got = self.kind_of_expr(v);
                    if !self.report_return_conflict(v, &got, w, v.span()) {
                        let got_s = self.typed_value_scalar(v);
                        self.report_numeric_return_mismatch(got_s, &got, i, v.span());
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
            let got = self.kind_of_expr(v);
            if !self.report_return_conflict(v, &got, w, v.span()) {
                let got_s = self.typed_value_scalar(v);
                self.report_numeric_return_mismatch(got_s, &got, i, v.span());
            }
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
        self.slice_element_type_name(first)
            .as_deref()
            .and_then(canonical_numeric_name)
    }

    /// #2884 — the canonical element scalar of a slice VALUE inside
    /// `append(dst, src...)`.
    ///
    /// `slice_element_type_name` reads the declared element text of a name
    /// (`var src []int64`), which is where the width survives without an
    /// `OpKind` that collapses it. A value bound to a CALL (`src := mk64()`)
    /// has no such text, so its callee's declared single result type is read
    /// instead — the shape the issue reports. Everything else fails open.
    fn append_elem_scalar_of_expr(&self, e: &Expr) -> Option<&'static str> {
        if let Some(text) = self.slice_element_type_name(e) {
            if let Some(scalar) = canonical_numeric_name(&text) {
                return Some(scalar);
            }
        }
        let Expr::Call { callee, .. } = unwrap_parens(e) else {
            return None;
        };
        let Expr::Ident { name, .. } = unwrap_parens(callee) else {
            return None;
        };
        let head = Checker::base_named_type_str(name);
        let f = self
            .ck
            .fns
            .get(name.as_str())
            .or_else(|| self.ck.fns.get(head))?;
        if !f.type_params.is_empty() || f.results.len() != 1 {
            return None;
        }
        let TypeExpr::Slice { elem, .. } = &f.results[0].ty else {
            return None;
        };
        let TypeExpr::Named { name, .. } = &**elem else {
            return None;
        };
        canonical_numeric_name(name)
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
                            (Some(d), Some(s)) => {
                                crate::const_overflow::canonical_int_type_name(d)
                                    != crate::const_overflow::canonical_int_type_name(s)
                            }
                            _ => false,
                        };
                        let kind_mismatch = elem != *extra_elem
                            && !matches!(elem, OpKind::Unknown)
                            && !matches!(*extra_elem, OpKind::Unknown);
                        // #2884: the declared-text comparison above cannot see a
                        // value bound to a CALL (`src := mk64()`), which is how
                        // `append(dst_int32, src_int64...)` escaped. The callee's
                        // result type carries that scalar.
                        let scalar_mismatch = matches!(
                            (
                                self.append_elem_scalar_of_expr(&first.value),
                                self.append_elem_scalar_of_expr(&extra.value)
                            ),
                            (Some(d), Some(s)) if d != s
                        );
                        if kinds_conflict(&elem, &extra_elem)
                            || kind_mismatch
                            || ty_mismatch
                            || scalar_mismatch
                        {
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
            if kinds_conflict(&elem, &got) || int_to_float_conflict || scalar_mismatch {
                let dst_label = match dst_elem_ty.as_deref() {
                    Some(d) => format!("[]{}", d),
                    None => self.kind_of_expr(&first.value).label().to_string(),
                };
                let got_label = match got_scalar {
                    Some(s) => s.to_string(),
                    None => got.label().to_string(),
                };
                let elem_label = match dst_scalar {
                    Some(d) => d.to_string(),
                    None => elem.label().to_string(),
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

    fn check_match_lit_pattern(&mut self, subject_kind: &OpKind, lit: &LitKind, span: Span) {
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
                        if conflict || scalar_mismatch {
                            self.emit_e1002(
                                v.span(),
                                format!(
                                    "cannot assign a value of type `{}` to `{}` (type `{}`) in parallel assignment — Go refuses `cannot use ... as {} value in assignment` (HYB-E1002)",
                                    vk.label(),
                                    t.render(),
                                    tpk.label(),
                                    tpk.label()
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
                let conflict = (**key_kind == OpKind::Float && *ik == OpKind::Int && !is_untyped_int_to_float)
                    || kinds_conflict(key_kind, ik);
                if conflict {
                    self.emit_e1002(
                        index.span(),
                        format!(
                            "cannot use `{}` (type `{}`) as `{}` value in map index on `{}` (HYB-E1002)",
                            index.render(),
                            ik.label(),
                            key_kind.label(),
                            bk.label()
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
        let is_array = matches!(bk, OpKind::Array(_))
            || matches!(bk, OpKind::Pointer(inner) if matches!(**inner, OpKind::Array(_)));
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
                    if let Some(arr_len) =
                        base_expr.and_then(|b| self.eval_const_array_len_at(b, span.start))
                    {
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
        let const_arr_len = if is_array {
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
        if expected == OpKind::Unknown {
            return;
        }
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
            // #2788: `ik == expected` collapses every integer width into
            // `Int` and both float widths into `Float`, so a TYPED element
            // whose concrete spelling differs (`int` into `int64`, `float64`
            // into `float32`) slipped through and failed `go build`
            // (`cannot use i (variable of type int) as int64 value in array
            // or slice literal`).
            let scalar_mismatch = self
                .scalar_names_conflict(self.scalar_of_ty(&resolved_ty), self.typed_value_scalar(item));
            if ik == OpKind::Unknown || expected == OpKind::Interface {
                continue;
            }
            if ik == expected && !scalar_mismatch {
                continue;
            }
            let is_untyped_int_to_float = expected == OpKind::Float
                && ik == OpKind::Int
                && self.is_untyped_int_const(item);
            let conflict = (expected == OpKind::Float && ik == OpKind::Int && !is_untyped_int_to_float)
                || kinds_conflict(&expected, &ik)
                || scalar_mismatch;
            if !conflict {
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

    fn check_slice_lit_items(&mut self, elem_ty: &TypeExpr, items: &[Expr]) {
        for item in items {
            self.check_single_value_position(item, "a slice literal element", false);
        }
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
            if self.non_single_value_count(item).is_some() {
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
                    self.check_slice_lit_items(sub_ty, sub_items);
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
            // #2788: same width-collapse escape as the array arm above.
            let scalar_mismatch = self
                .scalar_names_conflict(self.scalar_of_ty(&resolved_ty), self.typed_value_scalar(item));
            if ik == OpKind::Unknown || expected == OpKind::Interface {
                continue;
            }
            if ik == expected && !scalar_mismatch {
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
                || scalar_mismatch;
            if !conflict {
                continue;
            }
            self.emit_e1002(
                item.span(),
                format!(
                    "cannot use `{}` (type `{}`) as `{}` value in slice literal (HYB-E1002)",
                    item.render(),
                    ik.label(),
                    elem_ty.render()
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
            if exp_key != OpKind::Unknown && exp_key != OpKind::Interface && self.non_single_value_count(k).is_none() {
                let kk = self.kind_of_expr(k);
                // #2788: a typed key of another concrete numeric width.
                let scalar_mismatch = self
                    .scalar_names_conflict(self.scalar_of_ty(&resolved_key), self.typed_value_scalar(k));
                let is_untyped_int_to_float = exp_key == OpKind::Float
                    && kk == OpKind::Int
                    && self.is_untyped_int_const(k);
                let conflict = (exp_key == OpKind::Float && kk == OpKind::Int && !is_untyped_int_to_float)
                    || kinds_conflict(&exp_key, &kk)
                    || scalar_mismatch;
                if kk != OpKind::Unknown && conflict {
                    self.emit_e1002(
                        k.span(),
                        format!(
                            "cannot use `{}` (type `{}`) as `{}` value in map key (HYB-E1002)",
                            k.render(),
                            kk.label(),
                            key_ty.render()
                        ),
                        "every key of a map composite literal `map[K]V{...}` must be assignable to the key type `K`",
                    );
                }
            }
            if exp_val != OpKind::Unknown && exp_val != OpKind::Interface && self.non_single_value_count(v).is_none() {
                let bare_v = unwrap_parens(v);
                if matches!(bare_v, Expr::Lit { kind: LitKind::Nil, .. }) {
                    continue;
                }
                let vk = self.kind_of_expr(v);
                // #2788: a typed value of another concrete numeric width.
                let scalar_mismatch = self
                    .scalar_names_conflict(self.scalar_of_ty(&resolved_val), self.typed_value_scalar(v));
                let is_untyped_int_to_float = exp_val == OpKind::Float
                    && vk == OpKind::Int
                    && self.is_untyped_int_const(v);
                let conflict = (exp_val == OpKind::Float && vk == OpKind::Int && !is_untyped_int_to_float)
                    || kinds_conflict(&exp_val, &vk)
                    || scalar_mismatch;
                if vk != OpKind::Unknown && conflict {
                    self.emit_e1002(
                        v.span(),
                        format!(
                            "cannot use `{}` (type `{}`) as `{}` value in map value (HYB-E1002)",
                            v.render(),
                            vk.label(),
                            val_ty.render()
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
                        if let OpKind::Fn(_, Some(want)) = fk {
                            if want != args.len() {
                                self.report_local_arity(&callee.render(), want, args.len(), span);
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
                    if is_fully_resolved(&bk) && !is_lowered
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
        // #2701: among the built-ins only `append` is variadic; every other
        // built-in rejects the `...` spread form (`invalid use of ... with
        // built-in <name>`). `append` is handled in its own arm below, where a
        // spread argument is legal.
        if matches!(
            name,
            "len" | "cap" | "close" | "delete" | "min" | "max" | "clear" | "copy"
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
                    // #2474: `delete(s, v)` on a `set[T]` (lowered to
                    // `s.remove(v)`) needs `v` of type `T`.
                    OpKind::Unknown => {
                        if let Some(elem) = self.set_elem_of_expr(map_arg) {
                            let want = self.kind_of_named(&elem, 0);
                            let got = self.kind_of_expr(key_arg);
                            let int_to_float_conflict = want == OpKind::Float
                                && got == OpKind::Int
                                && !self.is_untyped_int_const(key_arg);
                            if kinds_conflict(&want, &got) || int_to_float_conflict {
                                self.emit_e2005(
                                    span,
                                    format!(
                                        "cannot use `{}` (type `{}`) as element type `{}` in `delete` on `set[{}]` (HYB-E2005)",
                                        key_arg.render(),
                                        got.label(),
                                        want.label(),
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
                        if **k_expected != OpKind::Unknown
                            && kk != OpKind::Unknown
                            && kk != **k_expected
                            && !is_untyped_int_to_float
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
            "copy" => {
                if args.len() != 2 {
                    return; // arity handled by builtins.rs
                }
                let dst_arg = &args[0].value;
                let src_arg = &args[1].value;
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
                        if copy_element_kinds_conflict(dst_elem, src_elem) {
                            self.emit_e1002(
                                span,
                                format!(
                                    "arguments to `copy` have different element types: `{}` and `{}` (HYB-E1002)",
                                    dst_elem.label(),
                                    src_elem.label()
                                ),
                                "arguments to `copy` must have identical element types",
                            );
                        }
                    }
                    (OpKind::Slice(dst_elem), OpKind::Str) => {
                        if !matches!(**dst_elem, OpKind::Unknown | OpKind::Int) {
                            self.emit_e1002(
                                span,
                                format!(
                                    "cannot copy string into slice with element type `{}` (HYB-E1002)",
                                    dst_elem.label()
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
            "min" | "max" => {
                let mut first_arg: Option<(&Expr, OpKind)> = None;
                for arg in args {
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
