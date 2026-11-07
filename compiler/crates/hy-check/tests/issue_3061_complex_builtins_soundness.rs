//! Regression test for Issue #3061:
//! `real`, `imag`, and `complex` operand kind soundness (HYB-E2005 / HYB-E1002).
//! Under GUARANTEES.md Row 13 (BUILDABLE), any program accepted by `hyb check`
//! must emit Go code that passes `go build`.

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

fn assert_reject(src: &str) {
    let cs = codes(src);
    assert!(
        !cs.is_empty() && cs.iter().any(|c| c == "HYB-E2005" || c == "HYB-E1002"),
        "expected HYB-E2005 or HYB-E1002, got {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(
        cs.is_empty(),
        "expected 0 diagnostics, got {cs:?} for:\n{src}"
    );
}

#[test]
fn test_real_rejects_string_var() {
    assert_reject(
        r#"
fn main() {
    s := "hello"
    println(s)
    _ = real(s)
}
"#,
    );
}

#[test]
fn test_real_rejects_bool_var() {
    assert_reject(
        r#"
fn main() {
    b := true
    println(b)
    _ = real(b)
}
"#,
    );
}

#[test]
fn test_real_rejects_slice_var() {
    assert_reject(
        r#"
fn main() {
    xs := []int{1, 2}
    println(xs)
    _ = real(xs)
}
"#,
    );
}

#[test]
fn test_real_rejects_map_var() {
    assert_reject(
        r#"
fn main() {
    m := map[string]int{}
    println(m)
    _ = real(m)
}
"#,
    );
}

#[test]
fn test_real_rejects_int_var() {
    assert_reject(
        r#"
fn main() {
    x := 1
    println(x)
    _ = real(x)
}
"#,
    );
}

#[test]
fn test_imag_rejects_string_var() {
    assert_reject(
        r#"
fn main() {
    s := "hello"
    println(s)
    _ = imag(s)
}
"#,
    );
}

#[test]
fn test_imag_rejects_bool_var() {
    assert_reject(
        r#"
fn main() {
    b := true
    println(b)
    _ = imag(b)
}
"#,
    );
}

#[test]
fn test_imag_rejects_int_var() {
    assert_reject(
        r#"
fn main() {
    x := 1
    println(x)
    _ = imag(x)
}
"#,
    );
}

#[test]
fn test_complex_rejects_string_var() {
    assert_reject(
        r#"
fn main() {
    s := "hello"
    println(s)
    _ = complex(s, s)
}
"#,
    );
}

#[test]
fn test_complex_rejects_bool_var() {
    assert_reject(
        r#"
fn main() {
    b := true
    println(b)
    _ = complex(b, b)
}
"#,
    );
}

#[test]
fn test_complex_rejects_int_var() {
    assert_reject(
        r#"
fn main() {
    x := 1
    println(x)
    _ = complex(x, x)
}
"#,
    );
}

#[test]
fn test_accepts_valid_complex_calls() {
    assert_ok("fn main() { _ = real(1) }");
    assert_ok("fn main() { _ = real(1.5) }");
    assert_ok("fn main() { _ = imag(2) }");
    assert_ok("fn main() { _ = imag(2.5) }");
    assert_ok("fn main() { _ = complex(1.0, 2.0) }");
    assert_ok("fn main() {\n    f1 := 1.0\n    f2 := 2.0\n    _ = complex(f1, f2)\n}");
}
