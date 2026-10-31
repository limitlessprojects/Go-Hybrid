//! typed_int_to_float_value_surfaces_1112.rs — regression for issue #1112.
//!
//! A typed `int` VARIABLE is not an untyped constant: Go refuses to convert
//! it to `float64`, on every value-bearing surface. Before the
//! value-provenance wrapper (`kinds_conflict_value`), hy-check accepted all
//! of these because the kind model blanket-allowed `(Float, Int)`:
//!
//!   1. `var` initializer        5. function argument
//!   2. array element            6. return statement
//!   3. `append`                 7. channel send
//!   4. map value                8. struct field assignment
//!
//! All reject fail-closed as HYB-E1002 (rule BUILDABLE). The accept controls
//! pin the Go-compatible behavior that motivated the fail-open design: an
//! untyped int constant (literal, or file-level `const` without type
//! ascription; parens/unary-minus preserve constantness) still converts, on
//! the same surfaces. Reproduced against go1.26.9: every reject shape fails
//! `go build` with 'cannot use ... as float64 value', every accept shape
//! builds.

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_reject(src: &str) {
    let found = codes(src);
    assert!(
        found.iter().any(|code| code == "HYB-E1002"),
        "expected HYB-E1002, got {found:?} for:\n{src}"
    );
}

fn assert_accept(src: &str) {
    let found = codes(src);
    assert!(
        found.is_empty(),
        "expected acceptance, got {found:?} for:\n{src}"
    );
}

// ---- rejects: one per surface, value is a typed int VARIABLE ----

#[test]
fn rejects_var_initializer_int_variable() {
    assert_reject(
        r#"fn main() {
    var i int = 3
    var x float64 = i
    fmt.println("{x}")
}
"#,
    );
}

#[test]
fn rejects_array_element_int_variable() {
    assert_reject(
        r#"fn main() {
    var a [2]float64
    var i int
    a[0] = i
    fmt.println("{a[0]}")
}
"#,
    );
}

#[test]
fn rejects_append_int_variable() {
    assert_reject(
        r#"fn main() {
    var s []float64
    var i int
    s = append(s, i)
    fmt.println("{s}")
}
"#,
    );
}

#[test]
fn rejects_map_value_int_variable() {
    assert_reject(
        r#"fn main() {
    var m map[string]float64
    var i int
    m["k"] = i
    fmt.println("{len(m)}")
}
"#,
    );
}

#[test]
fn rejects_fn_argument_int_variable() {
    assert_reject(
        r#"fn g(x f64) { print(x) }
fn main() {
    i := 2
    g(i)
}
"#,
    );
}

#[test]
fn rejects_return_int_variable() {
    assert_reject(
        r#"fn g(i int) f64 { return i }
fn main() {
    print(g(2))
}
"#,
    );
}

#[test]
fn rejects_channel_send_int_variable() {
    assert_reject(
        r#"fn main() {
    c := make(chan float64)
    i := 5
    c <- i
}
"#,
    );
}

#[test]
fn rejects_struct_field_int_variable() {
    assert_reject(
        r#"struct P { f f64 }
fn main() {
    var p P
    var y int
    p.f = y
    print(p.f)
}
"#,
    );
}

#[test]
fn rejects_short_decl_initializer_int_variable() {
    // `:=` infers the declared type from the value; the int variable keeps
    // its type on the way into a float64-typed sibling binding only if the
    // ascription says float64.
    assert_reject(
        r#"fn main() {
    var x f64
    y := 3
    x = y
    fmt.println("{x}")
}
"#,
    );
}

#[test]
fn rejects_compound_assign_int_variable() {
    assert_reject(
        r#"fn main() {
    var x float64
    var y int = 3
    x += y
    fmt.println("{x}")
}
"#,
    );
}

// ---- accepts: untyped int constants still convert (Go compatibility) ----

#[test]
fn accepts_literal_on_value_bearing_sites() {
    assert_accept(
        r#"fn g(x f64) { print(x) }
fn h() f64 { return 1 }
fn main() {
    var x float64 = 1
    var a [2]float64
    a[0] = 2
    var s []float64
    s = append(s, 3)
    var m map[string]float64
    m["k"] = 4
    g(5)
    print(h() + 1)
    c := make(chan float64)
    c <- 6
    print(<-c)
    fmt.println("{x}{a[0]}")
}
"#,
    );
}

#[test]
fn accepts_untyped_named_constant_and_parens_unary() {
    assert_accept(
        r#"const k = 3
fn main() {
    var x float64 = k
    x = (k)
    x = -k
    fmt.println("{x}")
}
"#,
    );
}

#[test]
fn accepts_matching_float_binding_and_explicit_conversion() {
    assert_accept(
        r#"fn g(x f64) { print(x) }
fn main() {
    var x float64
    var y float64 = 3
    x = y
    var n int = 2
    g(float64(n))
    fmt.println("{x}")
}
"#,
    );
}

#[test]
fn accepts_untyped_literal_into_int_target_stays_fine() {
    // The wrapper only excuses (Float, Int); (Int, Int) and (Int, Float)
    // targets keep their existing verdicts.
    assert_accept(
        r#"fn main() {
    var i int = 3
    i = 4
    fmt.println("{i}")
}
"#,
    );
}
