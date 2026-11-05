//! Tests for `new` type-operand and `print`/`println` void-builtin soundness.
//!
//! Validates:
//! 1. `new(T)` rejects value variables, constants, functions, and expressions under `HYB-E2005`.
//! 2. `print(...)` and `println(...)` return no value and are rejected when used in single-value
//!    positions (assignments, binary operations, call arguments) under `HYB-E1002`.
//! 3. Legal `make`, `new`, `print`, and `println` expressions are cleanly accepted.
//!
//! NOTE (in-merge fixup): the original submission also pinned make-size
//! validation (non-integer/negative-const/len>cap) under HYB-E2005. That rule
//! is already landed on main — `issue_2693_make_size_soundness.rs` (641a5514)
//! plus the classify_make_size E2005 checks — so the duplicate make arm was
//! removed here and the duplicate make tests deleted; see PR reply for the
//! full supersession evidence.

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
