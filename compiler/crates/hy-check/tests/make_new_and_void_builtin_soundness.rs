//! Tests for make, new, and print/println builtin operand soundness.
//!
//! Validates:
//! 1. `make([]T, len[, cap])`, `make(map[K]V, size)`, `make(chan T, buffer)` reject non-integer
//!    operand types (strings, bools, floats, etc.) under `HYB-E2005`.
//! 2. `make` rejects named constant float, string, and bool operands (`const f = 2.5`), named constant
//!    negative integers (`const c = -5`), and constant `len > cap` under `HYB-E2005`.
//! 3. `new(T)` rejects value variables, constants, functions, and expressions under `HYB-E2005`.
//! 4. `print(...)` and `println(...)` return no value and are rejected when used in single-value
//!    positions (assignments, binary operations, call arguments) under `HYB-E1002`.
//! 5. Legal `make`, `new`, `print`, and `println` expressions are cleanly accepted.

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

fn assert_has_code(src: &str, want: &str) {
    let cs = codes(src);
    assert!(
        cs.iter().any(|c| c == want),
        "expected [{want}] in diagnostics, got {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(cs.is_empty(), "expected 0 diagnostics, got {cs:?} for:\n{src}");
}

#[test]
fn make_slice_non_integer_variable_len_rejected() {
    let src = r#"
fn main() {
    s := "hello"
    xs := make([]int{}, s)
    _ = xs
}
"#;
    assert_has_code(src, "HYB-E2005");
}

#[test]
fn make_slice_bool_variable_len_rejected() {
    let src = r#"
fn main() {
    b := true
    xs := make([]int{}, b)
    _ = xs
}
"#;
    assert_has_code(src, "HYB-E2005");
}

#[test]
fn make_slice_float_variable_len_rejected() {
    let src = r#"
fn main() {
    var f float64 = 3.14
    xs := make([]int{}, f)
    _ = xs
}
"#;
    assert_has_code(src, "HYB-E2005");
}

#[test]
fn make_slice_non_integer_cap_rejected() {
    let src = r#"
fn main() {
    s := "hello"
    xs := make([]int{}, 10, s)
    _ = xs
}
"#;
    assert_has_code(src, "HYB-E2005");
}

#[test]
fn make_map_non_integer_size_rejected() {
    let src = r#"
fn main() {
    s := "hello"
    m := make(map[string]int{}, s)
    _ = m
}
"#;
    assert_has_code(src, "HYB-E2005");
}

#[test]
fn make_chan_non_integer_buffer_rejected() {
    let src = r#"
fn main() {
    s := "hello"
    ch := make(chan int, s)
    _ = ch
}
"#;
    assert_has_code(src, "HYB-E2005");
}

#[test]
fn make_named_const_float_and_negative_and_overflow_rejected() {
    let src_float = r#"
const cf = 2.5
fn main() {
    xs := make([]int{}, cf)
    _ = xs
}
"#;
    assert_has_code(src_float, "HYB-E2005");

    let src_neg = r#"
const cneg = -5
fn main() {
    xs := make([]int{}, cneg)
    _ = xs
}
"#;
    assert_has_code(src_neg, "HYB-E2005");

    let src_cap = r#"
const clen = 10
const ccap = 5
fn main() {
    xs := make([]int{}, clen, ccap)
    _ = xs
}
"#;
    assert_has_code(src_cap, "HYB-E2005");
}

#[test]
fn new_with_variable_or_expression_rejected() {
    let src_var = r#"
fn main() {
    s := "hello"
    p := new(s)
    _ = p
}
"#;
    assert_has_code(src_var, "HYB-E2005");

    let src_const = r#"
const c = 5
fn main() {
    p := new(c)
    _ = p
}
"#;
    assert_has_code(src_const, "HYB-E2005");

    let src_expr = r#"
fn main() {
    p := new(1 + 2)
    _ = p
}
"#;
    assert_has_code(src_expr, "HYB-E2005");

    let src_call = r#"
fn foo() int { return 1 }
fn main() {
    p := new(foo())
    _ = p
}
"#;
    assert_has_code(src_call, "HYB-E2005");
}

#[test]
fn print_and_println_void_used_as_value_rejected() {
    let src_println_binding = r#"
fn main() {
    x := println("hello")
    _ = x
}
"#;
    assert_has_code(src_println_binding, "HYB-E1002");

    let src_print_binding = r#"
fn main() {
    x := print("hello")
    _ = x
}
"#;
    assert_has_code(src_print_binding, "HYB-E1002");

    let src_println_binop = r#"
fn main() {
    x := 1 + println("hello")
    _ = x
}
"#;
    assert_has_code(src_println_binop, "HYB-E1002");
}

#[test]
fn legal_make_new_and_print_accepted() {
    let src = r#"
struct point {
    x int
    y int
}

fn main() {
    xs := make([]int{}, 5, 10)
    _ = xs

    m := make(map[string]int{}, 5)
    _ = m

    ch := make(chan int, 5)
    _ = ch

    pi := new(int)
    _ = pi

    ps := new(string)
    _ = ps

    pp := new(point)
    _ = pp

    px := new([]int{})
    _ = px

    println("hello")
    print("world")
}
"#;
    assert_ok(src);
}
