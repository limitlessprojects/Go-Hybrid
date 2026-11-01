//! #682 U9–U12 — operations on `any`.
//!
//! Four programs that `hyb check` accepted with 0 diagnostics while the emitted
//! Go failed `go build`, violating `GUARANTEES.md` Row 13 (BUILDABLE):
//!
//!   * **U9**  — `a + 1` where `a` is `any`: `invalid operation: a + 1
//!     (mismatched types any and untyped int)`.
//!   * **U10** — `a.nope` on `any`: `a.nope undefined (type any has no field or
//!     method nope)`.
//!   * **U11** — `a[0]` on `any`: `cannot index a (variable of interface type
//!     any)`.
//!   * **U12** — `a()` on `any`: `cannot call a (variable of interface type
//!     any)`.
//!
//! The root cause was a classification, not a missing check: `any` resolved to
//! `OpKind::Unknown`, and `Unknown` is deliberately fail-open everywhere (it
//! means "the checker could not resolve this"). `any` is the opposite — a
//! *resolved* type that Go refuses to operate on. Adding `OpKind::Interface`
//! puts it in the operand checks' reject set, because those checks list the
//! kinds they ACCEPT.
//!
//! The accept cases below are the reason this file is mostly controls. `any` is
//! legal in many positions — as an assignment target, a function parameter, a
//! task-spawn argument, a slice element, a map value, and an `==` operand — and
//! a rule that rejects those is a regression, not a fix. All nine were verified
//! to build before the classification changed, and `fmt.println(v)` in
//! particular is a Field access whose base is a package root, so it is the
//! first thing a careless field-access check would break.

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("parses");
    check_file(&ast)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn assert_code(src: &str, want: &str) {
    let cs = codes(src);
    assert!(
        !cs.is_empty() && cs.iter().all(|c| c == want),
        "expected only [{want}], got {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(cs.is_empty(), "expected 0 diagnostics, got {cs:?} for:\n{src}");
}

// ---------------------------------------------------------------------------
// U9 — arithmetic on `any`
// ---------------------------------------------------------------------------

#[test]
fn arithmetic_on_any_is_rejected() {
    assert_code(
        "fn main() {\n    var a any = 1\n    b := a + 1\n    fmt.println(\"{b}\")\n}\n",
        "HYB-E1002",
    );
    // The other arithmetic, bitwise, and shift operators are all undefined on
    // an interface.
    assert_code(
        "fn main() {\n    var a any = 1\n    b := a * 2\n    fmt.println(\"{b}\")\n}\n",
        "HYB-E1002",
    );
    assert_code(
        "fn main() {\n    var a any = 1\n    b := a << 1\n    fmt.println(\"{b}\")\n}\n",
        "HYB-E1002",
    );
    // Ordered comparison is not defined on `any` either.
    assert_code(
        "fn main() {\n    var a any = 1\n    if a < 2 {\n        fmt.println(\"lt\")\n    }\n}\n",
        "HYB-E1002",
    );
    // `&&` needs bool operands.
    assert_code(
        "fn main() {\n    var a any = true\n    if a && true {\n        fmt.println(\"ok\")\n    }\n}\n",
        "HYB-E1002",
    );
}

// ---------------------------------------------------------------------------
// U10 — field access on `any`
// ---------------------------------------------------------------------------

#[test]
fn field_access_on_any_is_rejected() {
    assert_code(
        "fn main() {\n    var a any = 1\n    b := a.nope\n    fmt.println(\"{b}\")\n}\n",
        "HYB-E1002",
    );
    // Calling a method on `any` is the same gap in a different position.
    assert_code(
        "fn main() {\n    var a any = 1\n    b := a.nope()\n    fmt.println(\"{b}\")\n}\n",
        "HYB-E1002",
    );
}

// ---------------------------------------------------------------------------
// U11 — index on `any`
// ---------------------------------------------------------------------------

#[test]
fn index_on_any_is_rejected() {
    assert_code(
        "fn main() {\n    var a any = 1\n    b := a[0]\n    fmt.println(\"{b}\")\n}\n",
        "HYB-E1002",
    );
}

// ---------------------------------------------------------------------------
// U12 — call of `any`
// ---------------------------------------------------------------------------

#[test]
fn call_of_any_is_rejected() {
    assert_code(
        "fn main() {\n    var a any = 1\n    b := a()\n    fmt.println(\"{b}\")\n}\n",
        "HYB-E1002",
    );
}

// ---------------------------------------------------------------------------
// the accept half
// ---------------------------------------------------------------------------

#[test]
fn any_as_a_value_stays_accepted() {
    // Assignment to an `any` target: an interface accepts any value.
    assert_ok("fn main() {\n    var a any = 1\n    a = \"str\"\n    fmt.println(\"{a}\")\n}\n");
    // `fmt.println(v)` — a Field access whose base is a package root, not an
    // interface. This is the false positive a careless check produces first.
    assert_ok("fn main() {\n    var v any = 42\n    fmt.println(\"{v}\")\n}\n");
    // Returning a concrete value from an `any` result.
    assert_ok(
        "fn box() any {\n    var g int = 1\n    return g\n}\n\nfn main() {\n    x := box()\n    fmt.println(\"{x}\")\n}\n",
    );
    // `any` as a slice element and a map value.
    assert_ok("fn main() {\n    xs := []any{1, \"two\"}\n    fmt.println(\"{len(xs)}\")\n}\n");
    assert_ok("fn main() {\n    m := map[string]any{}\n    m[\"k\"] = 1\n    fmt.println(\"{len(m)}\")\n}\n");
    // A struct field declared `any` takes any value.
    assert_ok(
        "struct s {\n    v any\n}\n\nfn main() {\n    var q s\n    q.v = 5\n    fmt.println(\"{q.v}\")\n}\n",
    );
}

#[test]
fn any_as_an_argument_stays_accepted() {
    // A plain call taking an `any` parameter.
    assert_ok("fn take(v any) {\n    fmt.println(\"taken\")\n}\n\nfn main() {\n    take(1)\n}\n");
    // The corpus's own control: `any` crossing a task boundary is classified by
    // the value behind the erasure, not by the erasing type (VERDICTS.md A1c).
    assert_ok(
        "fn worker(v any, done chan<- bool) {\n    done <- true\n}\n\nfn main() {\n    var v any = 42\n    done := make(chan bool, 1)\n    task worker(v, done)\n    <-done\n}\n",
    );
}

#[test]
fn equality_on_any_stays_accepted() {
    // Go DOES allow `==` between an interface and a non-interface value (the
    // non-interface side converts). Only the ordered comparisons are illegal.
    assert_ok(
        "fn main() {\n    var a any = 1\n    if a == 1 {\n        fmt.println(\"eq\")\n    }\n}\n",
    );
}


// ---------------------------------------------------------------------------
// #2962: implicit assignment of `any` to concrete type is rejected
// ---------------------------------------------------------------------------

#[test]
fn any_to_concrete_type_is_rejected() {
    // Variable initialization with `any` variable
    assert_code(
        "fn main() {\n    var a any = 1\n    var n int = a\n    fmt.println(\"{n}\")\n}\n",
        "HYB-E1002",
    );
    // Passing recover() (any) to concrete parameter
    assert_code(
        "fn take(s string) { fmt.println(s) }\nfn main() {\n    defer fn() {\n        take(recover())\n    }()\n    panic(\"boom\")\n}\n",
        "HYB-E1002",
    );
    // Returning recover() (any) as concrete return type
    assert_code(
        "fn get() string {\n    return recover()\n}\nfn main() {\n    fmt.println(get())\n}\n",
        "HYB-E1002",
    );
    // Initializing concrete slice literal with recover() (any)
    assert_code(
        "fn main() {\n    defer fn() {\n        r := recover()\n        xs := []string{r}\n        fmt.println(xs)\n    }()\n    panic(\"boom\")\n}\n",
        "HYB-E1002",
    );
    // Initializing int variable with recover() (any)
    assert_code(
        "fn main() {\n    r := recover()\n    var n int = r\n    fmt.println(\"{n}\")\n}\n",
        "HYB-E1002",
    );
}
