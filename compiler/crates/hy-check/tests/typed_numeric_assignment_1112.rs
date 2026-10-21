//! Regression for #1112: a typed `int` binding is not an untyped number.
//!
//! Keep the acceptance controls: Go permits untyped integer constants in
//! float64 assignments and compound assignments.
use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let file = parse_file("typed_numeric_assignment.hy", src).expect("valid Hybrid syntax");
    check_file(&file).diagnostics.into_iter().map(|d| d.code).collect()
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
    assert!(found.is_empty(), "expected acceptance, got {found:?} for:\n{src}");
}

#[test]
fn rejects_typed_local_integer_assignment() {
    assert_reject(
        r#"fn main() {
    var x float64
    var y int = 3
    x = y
    fmt.println("{x}")
}
"#,
    );
}

#[test]
fn rejects_typed_parameter_assignment() {
    assert_reject(
        r#"fn f(y int) {
    var x float64
    x = y
    fmt.println("{x}")
}
fn main() {
    f(3)
}
"#,
    );
}

#[test]
fn rejects_short_declared_integer_assignment() {
    assert_reject(
        r#"fn main() {
    var x float64
    y := 3
    x = y
    fmt.println("{x}")
}
"#,
    );
}

#[test]
fn rejects_typed_integer_compound_assignments() {
    for op in ["+=", "-=", "*=", "/="] {
        let src = r#"fn main() {
    var x float64 = 1
    var y int = 3
    x OP y
    fmt.println("{x}")
}
"#.replace("OP", op);
        assert_reject(&src);
    }
}

#[test]
fn accepts_untyped_numeric_literals() {
    assert_accept(
        r#"fn main() {
    var x float64 = 1
    x = 3
    x += 1
    x -= 1
    x *= 2
    x /= 2
    fmt.println("{x}")
}
"#,
    );
}

#[test]
fn accepts_untyped_named_constant() {
    assert_accept(
        r#"const c = 3
fn main() {
    var x float64
    x = c
    fmt.println("{x}")
}
"#,
    );
}

#[test]
fn accepts_matching_float_binding_and_explicit_conversion() {
    assert_accept(
        r#"fn main() {
    var x float64 = 1
    var y float64 = 3
    x = y
    x += y
    var n int = 2
    x = float64(n)
    fmt.println("{x}")
}
"#,
    );
}
