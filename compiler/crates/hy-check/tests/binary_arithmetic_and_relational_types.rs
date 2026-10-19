//! Regression test for mismatched operand types in binary arithmetic (`+`, `-`, `*`, `/`)
//! and relational comparisons (`<`, `<=`, `>`, `>=`).
//!
//! Go strictly forbids binary operations between different concrete numeric types (e.g. `int` and `float64`),
//! or between a typed `int` variable and a floating-point literal:
//!   `invalid operation: x + f (mismatched types int and float64)`
//!   `invalid operation: x < f (mismatched types int and float64)`
//!
//! At the same time, Go allows untyped numeric constants to adapt to floating targets (`f + 1`, `1 + f`, `f < 10`),
//! and allows constant expressions between untyped numeric literals (`1 + 2.5`, `1 < 2.5`).

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

fn assert_reject(src: &str, want_code: &str) {
    let cs = codes(src);
    assert!(
        cs.iter().any(|c| c == want_code),
        "expected error `{want_code}`, got: {cs:?}\nSource:\n{src}"
    );
}

fn assert_accept(src: &str) {
    let cs = codes(src);
    assert!(
        cs.is_empty(),
        "expected 0 diagnostics, got: {cs:?}\nSource:\n{src}"
    );
}

#[test]
fn reject_binary_arithmetic_int_and_float_variables() {
    assert_reject(
        r#"
package main
fn main() {
    x := 10
    var f float64 = 2.5
    _ = x + f
}
"#,
        "HYB-E1002",
    );

    assert_reject(
        r#"
package main
fn main() {
    x := 10
    var f float64 = 2.5
    _ = f + x
}
"#,
        "HYB-E1002",
    );

    assert_reject(
        r#"
package main
fn main() {
    x := 10
    var f float64 = 2.5
    _ = x - f
}
"#,
        "HYB-E1002",
    );

    assert_reject(
        r#"
package main
fn main() {
    x := 10
    var f float64 = 2.5
    _ = x * f
}
"#,
        "HYB-E1002",
    );

    assert_reject(
        r#"
package main
fn main() {
    x := 10
    var f float64 = 2.5
    _ = x / f
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn reject_binary_arithmetic_int_var_and_float_literal() {
    assert_reject(
        r#"
package main
fn main() {
    x := 10
    _ = x + 2.5
}
"#,
        "HYB-E1002",
    );

    assert_reject(
        r#"
package main
fn main() {
    x := 10
    _ = 2.5 + x
}
"#,
        "HYB-E1002",
    );

    assert_reject(
        r#"
package main
fn main() {
    x := 10
    _ = x - 2.5
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn reject_relational_comparisons_int_and_float() {
    assert_reject(
        r#"
package main
fn main() {
    x := 10
    var f float64 = 2.5
    _ = x < f
}
"#,
        "HYB-E1002",
    );

    assert_reject(
        r#"
package main
fn main() {
    x := 10
    var f float64 = 2.5
    _ = f <= x
}
"#,
        "HYB-E1002",
    );

    assert_reject(
        r#"
package main
fn main() {
    x := 10
    _ = x > 2.5
}
"#,
        "HYB-E1002",
    );

    assert_reject(
        r#"
package main
fn main() {
    x := 10
    _ = 2.5 >= x
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn accept_valid_untyped_constants_and_same_types() {
    // Untyped int adapts to float variable
    assert_accept(
        r#"
package main
fn main() {
    var f float64 = 2.5
    _ = f + 1
    _ = 1 + f
    _ = f - 1
    _ = f * 2
    _ = f / 2
    _ = f < 10
    _ = 1 < f
}
"#,
    );

    // Untyped int and float constants evaluate together
    assert_accept(
        r#"
package main
fn main() {
    _ = 1 + 2.5
    _ = 2.5 + 1
    _ = 1 < 2.5
    _ = 2.5 > 1
}
"#,
    );

    // Identical types
    assert_accept(
        r#"
package main
fn main() {
    x := 10
    y := 20
    _ = x + y
    _ = x < y

    var f1 float64 = 1.0
    var f2 float64 = 2.0
    _ = f1 + f2
    _ = f1 < f2

    s1 := "hello"
    s2 := "world"
    _ = s1 + s2
    _ = s1 < s2
}
"#,
    );
}
