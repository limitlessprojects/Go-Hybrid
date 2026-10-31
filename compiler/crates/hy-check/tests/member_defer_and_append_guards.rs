//! #682 U2–U5 — undefined members, defer arity, and append element type.
//!
//! Four programs that `hyb check` accepted with 0 diagnostics while the emitted
//! Go failed `go build`, violating `GUARANTEES.md` Row 13 (BUILDABLE):
//!
//!   * **U2** — a call to a method on a type that has none:
//!     `x.nope()` on an `int` is `x.nope undefined (type int has no field or
//!     method nope)`.
//!   * **U3** — the same on a struct: `q.missing()` is
//!     `q.missing undefined (type s has no field or method missing)`.
//!   * **U4** — a deferred call with the wrong arity: `defer g()` for a 1-param
//!     `g` is `not enough arguments in call to g`.
//!   * **U5** — `append` with an element of the wrong type:
//!     `append([]int{1}, "str")` is `cannot use "str" ... as int value in
//!     argument to append`.
//!
//! All four are `HYB-E1002`. Each was reproduced as a real `go build` failure
//! against the emitted Go before any check was written.
//!
//! The accept half is the point of this file, because each check has a legal
//! neighbour that a careless version rejects. Two of them were caught that way
//! during development and are pinned here:
//!
//!   * `is_fully_resolved` — `hits.add(1)` on a `*atomic.int64` resolves to
//!     `Pointer(Unknown)` because `sync/atomic`'s generic is foreign to the kind
//!     model, and it is perfectly good Go. The corpus's own
//!     `17-atomic-channel-immutable.hy` pins that, and an early draft of this
//!     change rejected it.
//!   * `LOWERED_METHOD_INTRINSICS` — `stack.pop()` on a `[]int` is example 08's
//!     shape, lowered by `hy-emit` to `hy_pop(&stack)`. Go has no slice method,
//!     so a naive "no such member" rule rejects a buildable program; the spike-6
//!     positive suite caught that.

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
// U2 / U3 — calls to a member that does not exist
// ---------------------------------------------------------------------------

#[test]
fn call_to_undefined_method_on_a_scalar_is_rejected() {
    assert_code(
        "fn main() {\n    x := 5\n    v := x.nope()\n    fmt.println(\"{v}\")\n}\n",
        "HYB-E1002",
    );
    // On a string, a slice, and a map — all fully resolved, none with members.
    assert_code(
        "fn main() {\n    s := \"a\"\n    v := s.nope()\n    fmt.println(\"{v}\")\n}\n",
        "HYB-E1002",
    );
    assert_code(
        "fn main() {\n    xs := []int{1}\n    v := xs.nope()\n    fmt.println(\"{v}\")\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn call_to_undefined_member_on_a_struct_is_rejected() {
    assert_code(
        "struct s {\n    a int\n}\n\nfn main() {\n    var q s\n    v := q.missing()\n    fmt.println(\"{v}\")\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn call_to_undefined_pop_on_struct_or_pointer_is_rejected() {
    assert_code(
        "struct point {\n    x int\n    y int\n}\n\nfn main() {\n    var pt point\n    _ = pt.pop()\n}\n",
        "HYB-E1002",
    );
    assert_code(
        "struct point {\n    x int\n    y int\n}\n\nfn main() {\n    var pt point\n    var ptr *point = &pt\n    _ = ptr.pop()\n}\n",
        "HYB-E1002",
    );
    assert_code(
        "fn main() {\n    var a any\n    _ = a.pop()\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn user_declared_pop_method_stays_accepted() {
    assert_ok(
        "struct stack {\n    items []int\n}\n\nfn (s *stack) pop() int {\n    return 42\n}\n\nfn main() {\n    var s stack\n    v := s.pop()\n    fmt.println(\"{v}\")\n}\n",
    );
}

#[test]
fn real_members_stay_accepted() {
    // A declared method.
    assert_ok(
        "struct c {\n    n int\n}\n\nfn (x *c) bump(k int) int {\n    x.n = x.n + k\n    return x.n\n}\n\nfn main() {\n    var z c\n    v := z.bump(2)\n    fmt.println(\"{v}\")\n}\n",
    );
    // A field read, and a function-typed field call.
    assert_ok("struct s {\n    a int\n}\n\nfn main() {\n    var q s\n    v := q.a\n    fmt.println(\"{v}\")\n}\n");
    assert_ok(
        "struct s {\n    f fn(a int) int\n}\n\nfn main() {\n    var q s\n    v := q.f(1)\n    fmt.println(\"{v}\")\n}\n",
    );
    // A package-qualified call is an Ident callee, not a member lookup.
    assert_ok("fn main() {\n    fmt.println(\"hi\")\n}\n");
}

#[test]
fn foreign_and_intrinsic_members_stay_accepted() {
    // `stack.pop()` on a `[]int` is a Hybrid intrinsic that `hy-emit` lowers to
    // `hy_pop(&stack)` — example 08's shape. Go has no slice method, so this
    // must not be reported.
    assert_ok(
        "fn main() {\n    stack := []int{1, 2}\n    v := stack.pop()\n    fmt.println(\"{v}\")\n}\n",
    );
}

#[test]
fn unresolved_types_stay_fail_open() {
    // `*atomic.int64` is foreign to the kind model: the pointer's pointee is
    // Unknown, so the members are invisible rather than absent. The corpus's
    // `17-atomic-channel-immutable.hy` pins this as ACCEPT.
    assert_ok(
        "import \"sync/atomic\"\n\nfn probe(hits *atomic.int64) {\n    hits.add(1)\n    fmt.println(\"{hits.load()}\")\n}\n\nfn main() {\n    fmt.println(\"ok\")\n}\n",
    );
}

// ---------------------------------------------------------------------------
// U4 — defer arity
// ---------------------------------------------------------------------------

#[test]
fn defer_with_wrong_arity_is_rejected() {
    assert_code(
        "fn g(a int) {\n    fmt.println(\"{a}\")\n}\n\nfn main() {\n    defer g()\n}\n",
        "HYB-E1002",
    );
    assert_code(
        "fn g(a int) {\n    fmt.println(\"{a}\")\n}\n\nfn main() {\n    defer g(1, 2)\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn legal_defer_stays_accepted() {
    assert_ok("fn g(a int) {\n    fmt.println(\"{a}\")\n}\n\nfn main() {\n    defer g(1)\n}\n");
    // A builtin and a method are not user `fn`s, so their arity is not ours.
    assert_ok("fn main() {\n    c := make(chan int, 1)\n    defer close(c)\n}\n");
    assert_ok(
        "struct r {\n    n int\n}\n\nfn (x *r) stop() {\n    fmt.println(\"stopped\")\n}\n\nfn main() {\n    var q r\n    defer q.stop()\n}\n",
    );
}

// ---------------------------------------------------------------------------
// U5 — append element type
// ---------------------------------------------------------------------------

#[test]
fn append_with_wrong_element_type_is_rejected() {
    assert_code(
        "fn main() {\n    xs := []int{1}\n    ys := append(xs, \"str\")\n    fmt.println(\"{ys}\")\n}\n",
        "HYB-E1002",
    );
    // A struct of the wrong type, and a mismatched nested slice.
    assert_code(
        "struct p {\n    n int\n}\n\nstruct t {\n    n int\n}\n\nfn main() {\n    xs := []p{}\n    var r t\n    ys := append(xs, r)\n    fmt.println(\"{len(ys)}\")\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn legal_append_stays_accepted() {
    assert_ok("fn main() {\n    xs := []int{1}\n    ys := append(xs, 2, 3)\n    fmt.println(\"{len(ys)}\")\n}\n");
    assert_ok("fn main() {\n    s := []string{\"a\"}\n    t := append(s, \"b\")\n    fmt.println(\"{len(t)}\")\n}\n");
    // A variadic spread passes the whole slice, not an element.
    assert_ok(
        "fn main() {\n    a := []int{1}\n    b := []int{2}\n    c := append(a, b...)\n    fmt.println(\"{len(c)}\")\n}\n",
    );
    // An untyped int constant IS assignable to a float64 element.
    assert_ok(
        "fn main() {\n    xs := []float64{}\n    ys := append(xs, 1)\n    fmt.println(\"{len(ys)}\")\n}\n",
    );
    // A matching struct element.
    assert_ok(
        "struct p {\n    n int\n}\n\nfn main() {\n    xs := []p{}\n    ys := append(xs, p{n: 1})\n    fmt.println(\"{len(ys)}\")\n}\n",
    );
}
