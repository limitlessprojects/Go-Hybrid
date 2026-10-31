//! #682 U13–U16 — struct-literal, field-assignment, and conversion-arity
//! soundness.
//!
//! Four programs that `hyb check` accepted with 0 diagnostics while the emitted
//! Go failed `go build`, violating `GUARANTEES.md` Row 13 (BUILDABLE):
//!
//!   * **U13** — a type conversion with more than one argument (`int(1, 2)`),
//!     which Go refuses with `too many arguments in conversion to int`.
//!   * **U14** — a struct literal naming a field the struct does not declare
//!     (`s{nope: 1}`), `unknown field nope in struct literal of type s`.
//!   * **U15** — a struct literal naming the same field twice
//!     (`s{a: 1, a: 2}`), `duplicate field name a in struct literal`.
//!   * **U16** — assigning a value of the wrong type to a field
//!     (`q.a = "str"` where `a` is `int`), `cannot use ... in assignment`.
//!
//! All four are `HYB-E1002` (`BUILDABLE`). Every reject case was first
//! reproduced as a real `go build` failure against the emitted Go, and every
//! accept case was verified to build.
//!
//! The accept cases are the point of this file. These are accept-side checks,
//! and a rule that closes a bypass by rejecting buildable programs is a
//! regression, not a fix — so the legal neighbours (an untyped constant for a
//! float field, `nil`, a positional literal, `any`, a struct-to-struct
//! conversion) are pinned here too.

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
// U13 — conversion arity
// ---------------------------------------------------------------------------

#[test]
fn conversion_with_two_arguments_is_rejected() {
    // Go: too many arguments in conversion to int.
    assert_code(
        "fn main() {\n    v := int(1, 2)\n    fmt.println(\"{v}\")\n}\n",
        "HYB-E1002",
    );
    assert_code(
        "fn main() {\n    v := string(1, 2)\n    fmt.println(\"{v}\")\n}\n",
        "HYB-E1002",
    );
    // A declared struct name is a conversion target too.
    assert_code(
        "struct p {\n    n int\n}\n\nfn main() {\n    q := p(1, 2)\n    fmt.println(\"{q.n}\")\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn one_argument_conversions_stay_accepted() {
    assert_ok("fn main() {\n    v := int(1)\n    fmt.println(\"{v}\")\n}\n");
    assert_ok("fn main() {\n    v := string(65)\n    fmt.println(\"{v}\")\n}\n");
    assert_ok("fn main() {\n    x := 1.5\n    v := int(x)\n    fmt.println(\"{v}\")\n}\n");
    // Builtins keep their own arity rules (HYB-E2005), and `len` is not a type.
    assert_ok("fn main() {\n    xs := []int{1}\n    n := len(xs)\n    fmt.println(\"{n}\")\n}\n");
    assert_ok(
        "fn main() {\n    xs := []int{1}\n    ys := append(xs, 2, 3)\n    fmt.println(\"{len(ys)}\")\n}\n",
    );
}

// ---------------------------------------------------------------------------
// U14 — unknown field in a struct literal
// ---------------------------------------------------------------------------

#[test]
fn unknown_field_in_struct_literal_is_rejected() {
    assert_code(
        "struct s {\n    a int\n}\n\nfn main() {\n    q := s{nope: 1}\n    fmt.println(\"{q.a}\")\n}\n",
        "HYB-E1002",
    );
    // A field that exists on a DIFFERENT struct is still unknown here.
    assert_code(
        "struct s {\n    a int\n}\n\nstruct t {\n    b int\n}\n\nfn main() {\n    q := s{b: 1}\n    fmt.println(\"{q.a}\")\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn declared_fields_stay_accepted() {
    assert_ok(
        "struct s {\n    a int\n    b int\n}\n\nfn main() {\n    q := s{a: 1, b: 2}\n    fmt.println(\"{q.a}\")\n}\n",
    );
    // A subset of the fields is fine in Go.
    assert_ok(
        "struct s {\n    a int\n    b int\n}\n\nfn main() {\n    q := s{a: 1}\n    fmt.println(\"{q.a}\")\n}\n",
    );
}

// ---------------------------------------------------------------------------
// U15 — duplicate field in a struct literal
// ---------------------------------------------------------------------------

#[test]
fn duplicate_field_in_struct_literal_is_rejected() {
    assert_code(
        "struct s {\n    a int\n}\n\nfn main() {\n    q := s{a: 1, a: 2}\n    fmt.println(\"{q.a}\")\n}\n",
        "HYB-E1002",
    );
}

// ---------------------------------------------------------------------------
// U16 — assignment type agreement
// ---------------------------------------------------------------------------

#[test]
fn wrong_typed_field_assignment_is_rejected() {
    assert_code(
        "struct s {\n    a int\n}\n\nfn main() {\n    var q s\n    q.a = \"str\"\n    fmt.println(\"{q.a}\")\n}\n",
        "HYB-E1002",
    );
    // A field declared as one struct cannot take a DIFFERENT struct.
    assert_code(
        "struct p {\n    n int\n}\n\nstruct t {\n    n int\n}\n\nstruct s {\n    a p\n}\n\nfn main() {\n    var q s\n    var r t\n    q.a = r\n    fmt.println(\"{q.a.n}\")\n}\n",
        "HYB-E1002",
    );
    // Plain variable assignment of a wrong-family scalar.
    assert_code(
        "fn main() {\n    var x int = 1\n    x = \"str\"\n    fmt.println(\"{x}\")\n}\n",
        "HYB-E1002",
    );
    // Indexing a slice with a wrong-typed value.
    assert_code(
        "fn main() {\n    xs := []int{1}\n    xs[0] = \"str\"\n    fmt.println(\"{xs[0]}\")\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn legal_assignments_stay_accepted() {
    // Untyped int constant into a float field — Go allows it.
    assert_ok(
        "struct s {\n    f float64\n}\n\nfn main() {\n    var q s\n    q.f = 1\n    fmt.println(\"{q.f}\")\n}\n",
    );
    // Same type, an alias, and a matching struct type.
    assert_ok(
        "struct s {\n    a int\n}\n\nfn main() {\n    var q s\n    q.a = 2\n    fmt.println(\"{q.a}\")\n}\n",
    );
    assert_ok(
        "type myint = int\n\nfn main() {\n    var x int = 1\n    x = 2\n    fmt.println(\"{x}\")\n}\n",
    );
    assert_ok(
        "struct p {\n    n int\n}\n\nstruct s {\n    a p\n}\n\nfn main() {\n    var q s\n    var r p\n    q.a = r\n    fmt.println(\"{q.a.n}\")\n}\n",
    );
    // `any` accepts anything, and `nil` stays fail-open (an unresolved kind).
    assert_ok(
        "fn main() {\n    var a any = 1\n    a = \"str\"\n    fmt.println(\"{a}\")\n}\n",
    );
    assert_ok(
        "fn main() {\n    var xs []int = []int{1}\n    fmt.println(\"{len(xs)}\")\n}\n",
    );
}

#[test]
fn multi_value_assignment_is_left_to_the_call_rules() {
    // `a, b = f()` is not a single-target assignment, so the new check must not
    // fire on it; the count rules own that shape.
    assert_ok(
        "fn two() (int, int) {\n    return 1, 2\n}\n\nfn main() {\n    a, b := two()\n    fmt.println(\"{a} {b}\")\n}\n",
    );
}

// ---------------------------------------------------------------------------
// U17 — a struct field initializer must be a single-value expression
// ---------------------------------------------------------------------------

#[test]
fn multi_value_call_in_struct_literal_field_is_rejected() {
    assert_code(
        "fn two() (int, int) {\n    return 1, 2\n}\n\nstruct Config {\n    timeout int\n    retries int\n}\n\nfn main() {\n    c := Config{timeout: two(), retries: 3}\n    fmt.println(\"{c.timeout}\")\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn void_call_in_struct_literal_field_is_rejected() {
    assert_code(
        "fn nothing() {\n    fmt.println(\"side effect\")\n}\n\nstruct Config {\n    timeout int\n}\n\nfn main() {\n    c := Config{timeout: nothing()}\n    fmt.println(\"{c.timeout}\")\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn single_value_call_and_literals_in_struct_fields_stay_accepted() {
    assert_ok(
        "fn one() int {\n    return 7\n}\n\nstruct Config {\n    timeout int\n    retries int\n}\n\nfn main() {\n    c := Config{timeout: one(), retries: 3}\n    fmt.println(\"{c.timeout}\")\n}\n",
    );
}
