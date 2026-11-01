//! #3083 — Typed int/float values and width mismatches must not pass check for complex targets/operands.
//!
//! In Go, complex types (`complex64`, `complex128`) are distinct numeric types that cannot
//! implicitly take or operate with typed integer or floating-point values:
//!   - `c = i` and `c = f` fail go build with `cannot use ... as complex128 value in assignment`
//!   - `return i` and `return f` fail go build with `cannot use ... as complex128 value in return statement`
//!   - `takes_c(i)` and `takes_c(f)` fail go build with `cannot use ... as complex128 value in argument`
//!   - `var c complex128 = i` fails go build with `cannot use ... as complex128 value in variable declaration`
//!   - `c - i`, `c * f` fail go build with `invalid operation: mismatched types complex128 and ...`
//!   - `c == i`, `c == f` fail go build with `invalid operation: mismatched types complex128 and ...`
//!   - `c128 = c64` and `c128 + c64` fail go build with mismatched types complex128 and complex64
//! Only untyped constants (`1`, `2.5`, `-3`) are assignable to and operatable with complex types.
//!
//! This suite verifies:
//!   - Typed int/float variable returns from complex functions are rejected (HYB-E1002).
//!   - Typed int/float variable assignments to complex are rejected (HYB-E1002).
//!   - Typed int/float variable declarations to complex are rejected (HYB-E1002).
//!   - Typed int/float call arguments to complex parameters are rejected (HYB-E1002).
//!   - Binary arithmetic and equality between complex and typed int/float are rejected (HYB-E1002).
//!   - Width mismatches between complex64 and complex128 are rejected (HYB-E1002).
//!   - Untyped constants and unary negation on complex stay accepted (0 diagnostics).

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

#[test]
fn test_rejects_typed_int_return_from_complex() {
    let src = r#"
fn f(i int) complex128 {
    return i
}
"#;
    let c = codes(src);
    assert!(
        c.iter().any(|code| code == "HYB-E1002"),
        "expected HYB-E1002 for returning typed int from complex128 function, got: {:?}",
        c
    );
}

#[test]
fn test_rejects_typed_float_return_from_complex() {
    let src = r#"
fn f(x float64) complex128 {
    return x
}
"#;
    let c = codes(src);
    assert!(
        c.iter().any(|code| code == "HYB-E1002"),
        "expected HYB-E1002 for returning typed float64 from complex128 function, got: {:?}",
        c
    );
}

#[test]
fn test_rejects_complex64_return_from_complex128() {
    let src = r#"
fn f(c complex64) complex128 {
    return c
}
"#;
    let c = codes(src);
    assert!(
        c.iter().any(|code| code == "HYB-E1002"),
        "expected HYB-E1002 for returning complex64 from complex128 function, got: {:?}",
        c
    );
}

#[test]
fn test_rejects_typed_int_assign_to_complex() {
    let src = r#"
fn main() {
    var c complex128
    var i int = 1
    c = i
    _ = c
}
"#;
    let c = codes(src);
    assert!(
        c.iter().any(|code| code == "HYB-E1002"),
        "expected HYB-E1002 for assigning typed int to complex128, got: {:?}",
        c
    );
}

#[test]
fn test_rejects_typed_float_assign_to_complex() {
    let src = r#"
fn main() {
    var c complex128
    var f float64 = 2.0
    c = f
    _ = c
}
"#;
    let c = codes(src);
    assert!(
        c.iter().any(|code| code == "HYB-E1002"),
        "expected HYB-E1002 for assigning typed float64 to complex128, got: {:?}",
        c
    );
}

#[test]
fn test_rejects_complex64_assign_to_complex128() {
    let src = r#"
fn main() {
    var c128 complex128
    var c64 complex64
    c128 = c64
    _ = c128
}
"#;
    let c = codes(src);
    assert!(
        c.iter().any(|code| code == "HYB-E1002"),
        "expected HYB-E1002 for assigning complex64 to complex128, got: {:?}",
        c
    );
}

#[test]
fn test_rejects_typed_int_init_complex() {
    let src = r#"
fn main() {
    var i int = 1
    var c complex128 = i
    _ = c
}
"#;
    let c = codes(src);
    assert!(
        c.iter().any(|code| code == "HYB-E1002"),
        "expected HYB-E1002 for initializing complex128 with typed int variable, got: {:?}",
        c
    );
}

#[test]
fn test_rejects_typed_float_init_complex() {
    let src = r#"
fn main() {
    var f float64 = 2.0
    var c complex128 = f
    _ = c
}
"#;
    let c = codes(src);
    assert!(
        c.iter().any(|code| code == "HYB-E1002"),
        "expected HYB-E1002 for initializing complex128 with typed float64 variable, got: {:?}",
        c
    );
}

#[test]
fn test_rejects_typed_int_argument_to_complex_param() {
    let src = r#"
fn takes_c(c complex128) {}

fn main() {
    var i int = 1
    takes_c(i)
}
"#;
    let c = codes(src);
    assert!(
        c.iter().any(|code| code == "HYB-E1002"),
        "expected HYB-E1002 for passing typed int to complex128 parameter, got: {:?}",
        c
    );
}

#[test]
fn test_rejects_typed_float_argument_to_complex_param() {
    let src = r#"
fn takes_c(c complex128) {}

fn main() {
    var f float64 = 2.0
    takes_c(f)
}
"#;
    let c = codes(src);
    assert!(
        c.iter().any(|code| code == "HYB-E1002"),
        "expected HYB-E1002 for passing typed float64 to complex128 parameter, got: {:?}",
        c
    );
}

#[test]
fn test_rejects_binary_arithmetic_complex_and_typed_int() {
    let src = r#"
fn main() {
    var c complex128
    var i int = 1
    _ = c - i
}
"#;
    let c = codes(src);
    assert!(
        c.iter().any(|code| code == "HYB-E1002"),
        "expected HYB-E1002 for `c - i` (complex128 and int), got: {:?}",
        c
    );
}

#[test]
fn test_rejects_binary_arithmetic_complex_and_typed_float() {
    let src = r#"
fn main() {
    var c complex128
    var f float64 = 2.0
    _ = c * f
}
"#;
    let c = codes(src);
    assert!(
        c.iter().any(|code| code == "HYB-E1002"),
        "expected HYB-E1002 for `c * f` (complex128 and float64), got: {:?}",
        c
    );
}

#[test]
fn test_rejects_equality_complex_and_typed_int() {
    let src = r#"
fn main() {
    var c complex128
    var i int = 1
    _ = c == i
}
"#;
    let c = codes(src);
    assert!(
        c.iter().any(|code| code == "HYB-E1002"),
        "expected HYB-E1002 for `c == i` (complex128 and int), got: {:?}",
        c
    );
}

#[test]
fn test_rejects_equality_complex_and_typed_float() {
    let src = r#"
fn main() {
    var c complex128
    var f float64 = 2.0
    _ = c == f
}
"#;
    let c = codes(src);
    assert!(
        c.iter().any(|code| code == "HYB-E1002"),
        "expected HYB-E1002 for `c == f` (complex128 and float64), got: {:?}",
        c
    );
}

#[test]
fn test_rejects_binary_arithmetic_complex128_and_complex64() {
    let src = r#"
fn main() {
    var c128 complex128
    var c64 complex64
    _ = c128 + c64
}
"#;
    let c = codes(src);
    assert!(
        c.iter().any(|code| code == "HYB-E1002"),
        "expected HYB-E1002 for `c128 + c64` (complex128 and complex64), got: {:?}",
        c
    );
}

#[test]
fn test_accepts_valid_untyped_constants_and_operations() {
    let src = r#"
fn ret_int_const() complex128 {
    return 1
}

fn ret_float_const() complex128 {
    return 2.5
}

fn takes_c(c complex128) {}

fn main() {
    var c complex128 = 1
    var c2 complex128 = 2.5
    takes_c(1)
    takes_c(2.5)
    c = 3
    c = 4.5
    _ = c + 1
    _ = c - 2.5
    _ = c * 3
    _ = c / 4.0
    _ = c == 1
    _ = c != 2.5
    _ = -c
    _ = c2
}
"#;
    let c = codes(src);
    assert!(
        c.is_empty(),
        "expected 0 diagnostics for valid untyped constants and operations on complex, got: {:?}",
        c
    );
}
