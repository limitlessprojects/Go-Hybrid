//! #577 C9/C10 — call arity and return-statement soundness.
//!
//! The Go-toolchain oracle in PR #577 measured a **38.03% accept-side
//! residual** on generated programs and named `call_and_return` as its worst
//! family (**63.33%**: 19 of 30 accepted programs failed `go build`). Two of
//! its ten enumerated classes are closed here:
//!
//!   * **C9** — a call whose argument count disagrees with the callee
//!     (`add(1)` / `add(1, 2, 3)` for `fn add(a int, b int) int`), including
//!     methods and closures.
//!   * **C10** — a `return` that does not match the enclosing signature:
//!     a wrong value count, or a value whose type cannot be assigned to its
//!     declared result.
//!
//! Both are `HYB-E1002` (`BUILDABLE`, `GUARANTEES.md` Row 13): the emitted Go
//! is rejected by `go build`, so an accepted program would not build.
//!
//! Every reject case below was first reproduced as a real `go build` failure
//! against the emitted Go, and every accept case below was verified to build.
//! The accept cases are the point of this file: this is an accept-side check,
//! and a rule that closes a bypass by rejecting buildable programs is a
//! regression, not a fix.

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

/// Exactly one diagnostic, of `want`.
fn assert_code(src: &str, want: &str) {
    let cs = codes(src);
    assert_eq!(cs, vec![want.to_string()], "for:\n{src}");
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(cs.is_empty(), "expected 0 diagnostics, got {cs:?} for:\n{src}");
}

// ---------------------------------------------------------------------------
// C9 — call arity
// ---------------------------------------------------------------------------

#[test]
fn top_level_call_arity_mismatch_is_rejected() {
    // `add(1)` — Go: not enough arguments in call to add.
    assert_code(
        "fn add(a int, b int) int {\n    return a + b\n}\n\nfn main() {\n    r := add(1)\n    fmt.println(\"{r}\")\n}\n",
        "HYB-E1002",
    );
    // `add(1, 2, 3)` — Go: too many arguments in call to add.
    assert_code(
        "fn add(a int, b int) int {\n    return a + b\n}\n\nfn main() {\n    r := add(1, 2, 3)\n    fmt.println(\"{r}\")\n}\n",
        "HYB-E1002",
    );
    // The same call nested inside a larger expression.
    assert_code(
        "fn add(a int, b int) int {\n    return a + b\n}\n\nfn main() {\n    r := add(1) + 2\n    fmt.println(\"{r}\")\n}\n",
        "HYB-E1002",
    );
    // A zero-parameter function called with one argument.
    assert_code(
        "fn f() int {\n    return 1\n}\n\nfn main() {\n    r := f(1)\n    fmt.println(\"{r}\")\n}\n",
        "HYB-E1002",
    );
    // A void function called with an argument.
    assert_code(
        "fn f() {\n    fmt.println(\"hi\")\n}\n\nfn main() {\n    f(1)\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn method_call_arity_mismatch_is_rejected() {
    // The receiver is not an argument: `bump(k int)` takes one.
    assert_code(
        "struct g {\n    n int\n}\n\nfn (x *g) bump(k int) int {\n    x.n = x.n + k\n    return x.n\n}\n\nfn main() {\n    var x g\n    r := x.bump()\n    fmt.println(\"{r}\")\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn closure_and_fn_typed_call_arity_mismatch_is_rejected() {
    // A closure bound to a local states its own parameter count.
    assert_code(
        "fn main() {\n    f := fn(x int) int {\n        return x + 1\n    }\n    r := f(1, 2)\n    fmt.println(\"{r}\")\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn correct_arity_stays_accepted() {
    // The controls for every reject above: same shapes, right count.
    assert_ok(
        "fn add(a int, b int) int {\n    return a + b\n}\n\nfn main() {\n    r := add(1, 2)\n    fmt.println(\"{r}\")\n}\n",
    );
    assert_ok("fn f() int {\n    return 1\n}\n\nfn main() {\n    r := f()\n    fmt.println(\"{r}\")\n}\n");
    assert_ok("fn f() {\n    fmt.println(\"hi\")\n}\n\nfn main() {\n    f()\n}\n");
    assert_ok(
        "struct g {\n    n int\n}\n\nfn (x *g) bump(k int) int {\n    x.n = x.n + k\n    return x.n\n}\n\nfn main() {\n    var x g\n    r := x.bump(2)\n    fmt.println(\"{r}\")\n}\n",
    );
    assert_ok(
        "fn main() {\n    f := fn(x int) int {\n        return x + 1\n    }\n    r := f(1)\n    fmt.println(\"{r}\")\n}\n",
    );
    // A builtin's arity is `builtins.rs`'s (HYB-E2005), not this rule's, and a
    // variadic builtin such as `fmt.println` takes any number of arguments.
    assert_ok("fn main() {\n    xs := []int{1, 2}\n    n := len(xs)\n    fmt.println(\"{n}\", \"and\", \"more\")\n}\n");
}

// ---------------------------------------------------------------------------
// C10 — return value count
// ---------------------------------------------------------------------------

#[test]
fn return_value_count_mismatch_is_rejected() {
    // A value in a function that declares no results.
    assert_code("fn f() {\n    return 1\n}\n\nfn main() {\n    f()\n}\n", "HYB-E1002");
    // No value in a function that declares one unnamed result.
    assert_code(
        "fn f() int {\n    return\n}\n\nfn main() {\n    x := f()\n    fmt.println(\"{x}\")\n}\n",
        "HYB-E1002",
    );
    // One value for two declared results (no `error` tail to zero-fill).
    assert_code(
        "fn f() (int, int) {\n    return 1\n}\n\nfn main() {\n    a, b := f()\n    fmt.println(\"{a} {b}\")\n}\n",
        "HYB-E1002",
    );
    // Two values for one declared result.
    assert_code(
        "fn f() int {\n    return 1, 2\n}\n\nfn main() {\n    x := f()\n    fmt.println(\"{x}\")\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn multi_value_call_used_as_the_whole_return_is_accepted() {
    // Go's rule: `return f()` is legal when f's results line up exactly.
    assert_ok(
        "fn two() (int, int) {\n    return 1, 2\n}\n\nfn f() (int, int) {\n    return two()\n}\n\nfn main() {\n    a, b := f()\n    fmt.println(\"{a} {b}\")\n}\n",
    );
}

#[test]
fn naked_return_with_all_named_results_is_accepted() {
    // Go's naked return: legal exactly when every result is named. Corpus 63
    // pins the closure form (`fn() (n int) { n = 7; return }`).
    assert_ok("fn f() (n int) {\n    n = 7\n    return\n}\n\nfn main() {\n    fmt.println(\"{f()}\")\n}\n");
    assert_ok(
        "fn main() {\n    f := fn() (n int) {\n        n = 7\n        return\n    }\n    fmt.println(\"{f()}\")\n}\n",
    );
}

#[test]
fn short_return_in_an_error_tail_fn_is_accepted() {
    // E3's sugar: the emitter zero-fills the remaining slots, so `return v` in
    // a `(value, error)` function emits `return v, nil`. Corpus 13 pins it.
    assert_ok(
        "fn f() (int, error) {\n    return 1\n}\n\nfn main() {\n    x, e := f()\n    fmt.println(\"{x} {e}\")\n}\n",
    );
}

// ---------------------------------------------------------------------------
// C10 — return value type
// ---------------------------------------------------------------------------

#[test]
fn returned_scalar_of_the_wrong_family_is_rejected() {
    assert_code(
        "fn f() int {\n    return \"s\"\n}\n\nfn main() {\n    x := f()\n    fmt.println(\"{x}\")\n}\n",
        "HYB-E1002",
    );
    assert_code(
        "fn f() string {\n    return 1\n}\n\nfn main() {\n    x := f()\n    fmt.println(\"{x}\")\n}\n",
        "HYB-E1002",
    );
    assert_code(
        "fn f() int {\n    return true\n}\n\nfn main() {\n    x := f()\n    fmt.println(\"{x}\")\n}\n",
        "HYB-E1002",
    );
    assert_code(
        "fn f() bool {\n    return 1\n}\n\nfn main() {\n    x := f()\n    fmt.println(\"{x}\")\n}\n",
        "HYB-E1002",
    );
    // A float VARIABLE into an int result (`return 1.5` as a literal is the
    // const-overflow rule's, HYB-E6002).
    assert_code(
        "fn f() int {\n    var g float64 = 1.5\n    return g\n}\n\nfn main() {\n    x := f()\n    fmt.println(\"{x}\")\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn returned_container_of_the_wrong_shape_is_rejected() {
    // A slice of the wrong element family.
    assert_code(
        "fn f() []int {\n    xs := []string{\"a\"}\n    return xs\n}\n\nfn main() {\n    x := f()\n    fmt.println(\"{len(x)}\")\n}\n",
        "HYB-E1002",
    );
    // A map where a slice is declared.
    assert_code(
        "fn f() []int {\n    m := map[int]int{1: 2}\n    return m\n}\n\nfn main() {\n    x := f()\n    fmt.println(\"{len(x)}\")\n}\n",
        "HYB-E1002",
    );
    // A different struct type.
    assert_code(
        "struct p {\n    n int\n}\n\nstruct q {\n    n int\n}\n\nfn f() p {\n    var r q\n    return r\n}\n\nfn main() {\n    x := f()\n    fmt.println(\"{x.n}\")\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn return_type_controls_stay_accepted() {
    // An untyped int constant is assignable to a float result.
    assert_ok(
        "fn f() float64 {\n    return 1\n}\n\nfn main() {\n    x := f()\n    fmt.println(\"{x}\")\n}\n",
    );
    // The same type on both sides, including an alias and an int8 result.
    assert_ok(
        "fn f() int {\n    var g int = 3\n    return g\n}\n\nfn main() {\n    x := f()\n    fmt.println(\"{x}\")\n}\n",
    );
    assert_ok(
        "fn f() int8 {\n    return 100\n}\n\nfn main() {\n    x := f()\n    fmt.println(\"{x}\")\n}\n",
    );
    assert_ok(
        "type myint = int\n\nfn f() myint {\n    var g int = 3\n    return g\n}\n\nfn main() {\n    x := f()\n    fmt.println(\"{x}\")\n}\n",
    );
    // Matching container shapes.
    assert_ok(
        "fn f() []int {\n    xs := []int{1}\n    return xs\n}\n\nfn main() {\n    x := f()\n    fmt.println(\"{len(x)}\")\n}\n",
    );
    assert_ok(
        "struct p {\n    n int\n}\n\nfn f() p {\n    var q p\n    return q\n}\n\nfn main() {\n    x := f()\n    fmt.println(\"{x.n}\")\n}\n",
    );
    // An interface result accepts anything, and a bare `return` in a void
    // function stays legal.
    assert_ok(
        "fn f() any {\n    var g int = 1\n    return g\n}\n\nfn main() {\n    x := f()\n    fmt.println(\"{x}\")\n}\n",
    );
    assert_ok("fn f() {\n    return\n}\n\nfn main() {\n    f()\n}\n");
}

#[test]
fn top_level_call_argument_type_mismatch_is_rejected() {
    // Calling with a string when int is declared.
    assert_code(
        "fn add(a int, b int) int {\n    return a + b\n}\n\nfn main() {\n    _ = add(\"hello\", 2)\n}\n",
        "HYB-E1002",
    );
    // Calling with a bool when string is declared.
    assert_code(
        "fn greet(msg string) {\n}\n\nfn main() {\n    greet(true)\n}\n",
        "HYB-E1002",
    );
    // Passing a slice when a map is declared.
    assert_code(
        "fn lookup(m map[string]int) int {\n    return 0\n}\n\nfn main() {\n    _ = lookup([]string{\"a\"})\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn method_call_argument_type_mismatch_is_rejected() {
    // Method argument type mismatch.
    assert_code(
        "struct calc {\n}\n\nfn (c calc) compute(a int) int {\n    return a * 2\n}\n\nfn main() {\n    var c calc\n    _ = c.compute(\"hello\")\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn task_spawn_argument_type_and_arity_mismatch_is_rejected() {
    // Task argument type mismatch.
    assert_code(
        "fn worker(n int) {\n}\n\nfn main() {\n    task worker(\"not_an_int\")\n}\n",
        "HYB-E1002",
    );
    // Task arity mismatch.
    assert_code(
        "fn worker(n int) {\n}\n\nfn main() {\n    task worker(1, 2)\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn call_argument_types_stay_accepted() {
    // Exact types.
    assert_ok(
        "fn add(a int, b int) int {\n    return a + b\n}\n\nfn main() {\n    _ = add(1, 2)\n}\n",
    );
    // Untyped int to float target is legal.
    assert_ok(
        "fn scale(f float64) float64 {\n    return f\n}\n\nfn main() {\n    _ = scale(2)\n}\n",
    );
    // Method with matching argument types.
    assert_ok(
        "struct calc {\n}\n\nfn (c calc) compute(a int) int {\n    return a * 2\n}\n\nfn main() {\n    var c calc\n    _ = c.compute(10)\n}\n",
    );
    // Any interface parameter accepts any value.
    assert_ok(
        "fn log_any(v any) {\n}\n\nfn main() {\n    log_any(123)\n    log_any(\"str\")\n}\n",
    );
}

#[test]
fn fn_type_local_and_struct_field_soundness_issue_727() {
    assert_ok(
        "package main\n\nimport \"fmt\"\n\nfn main() {\n    var cb fn() int\n    fmt.println(cb == nil)\n}\n",
    );
    assert_ok("package main\n\nfn main() {\n    var f fn(int) int\n    _ = f(1)\n}\n");
    assert_ok(
        "package main\n\nstruct handler {\n    on_event fn(int) bool\n    name string\n}\n\nfn main() {\n    h := handler{name: \"click\"}\n    _ = h.name\n}\n",
    );
    assert_code(
        "package main\n\nfn main() {\n    var f fn(int) int\n    _ = f(1, 2)\n}\n",
        "HYB-E1002",
    );
    assert_code(
        "package main\n\nfn main() {\n    var f fn(u8) int\n    _ = f(999)\n}\n",
        "HYB-E6002",
    );
    assert_code(
        "package main\n\ntype cb = fn(u8) int\n\nfn main() {\n    var f cb\n    _ = f(999)\n}\n",
        "HYB-E6002",
    );
    assert_code(
        "package main\n\nstruct handler {\n    on_event fn(u8) bool\n}\n\nfn main() {\n    var h handler\n    _ = h.on_event(999)\n}\n",
        "HYB-E6002",
    );
    assert!(
        codes("package main\n\nfn main() {\n    var f fn([no_such_const]int) int\n    _ = f\n}\n")
            .contains(&"HYB-E1001".to_string())
    );
}

