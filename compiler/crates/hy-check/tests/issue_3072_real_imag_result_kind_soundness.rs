//! Regression tests for Issue #3072:
//! [soundness] hy-check: real() and imag() result kinds are Unknown, letting mismatched returns, assignments, and operands pass check (Row 13 BUILDABLE).
//!
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

fn assert_e1002(src: &str) {
    let cs = codes(src);
    assert!(
        !cs.is_empty() && cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {cs:?} for:\n{src}"
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
fn test_rejects_mismatched_real_return_to_string() {
    assert_e1002(
        r#"
fn f(c complex128) string {
    return real(c)
}
fn main() {}
"#,
    );
}

#[test]
fn test_rejects_mismatched_imag_return_to_string() {
    assert_e1002(
        r#"
fn f(c complex128) string {
    return imag(c)
}
fn main() {}
"#,
    );
}

#[test]
fn test_rejects_mismatched_real_var_decl() {
    assert_e1002(
        r#"
fn main() {
    c := complex(1.0, 2.0)
    var s string = real(c)
    _ = s
}
"#,
    );
}

#[test]
fn test_rejects_mismatched_imag_var_decl() {
    assert_e1002(
        r#"
fn main() {
    c := complex(1.0, 2.0)
    var s string = imag(c)
    _ = s
}
"#,
    );
}

#[test]
fn test_rejects_mismatched_real_assignment() {
    assert_e1002(
        r#"
fn main() {
    c := complex(1.0, 2.0)
    var s string = "init"
    s = real(c)
    _ = s
}
"#,
    );
}

#[test]
fn test_rejects_mismatched_imag_assignment() {
    assert_e1002(
        r#"
fn main() {
    c := complex(1.0, 2.0)
    var s string = "init"
    s = imag(c)
    _ = s
}
"#,
    );
}

#[test]
fn test_rejects_mismatched_real_call_argument() {
    assert_e1002(
        r#"
fn take_str(s string) {}
fn main() {
    c := complex(1.0, 2.0)
    take_str(real(c))
}
"#,
    );
}

#[test]
fn test_rejects_mismatched_imag_call_argument() {
    assert_e1002(
        r#"
fn take_str(s string) {}
fn main() {
    c := complex(1.0, 2.0)
    take_str(imag(c))
}
"#,
    );
}

#[test]
fn test_rejects_mismatched_real_binary_add_to_string() {
    assert_e1002(
        r#"
fn main() {
    c := complex(1.0, 2.0)
    _ = real(c) + "bad"
}
"#,
    );
}

#[test]
fn test_rejects_mismatched_imag_binary_add_to_string() {
    assert_e1002(
        r#"
fn main() {
    c := complex(1.0, 2.0)
    _ = imag(c) + "bad"
}
"#,
    );
}

#[test]
fn test_accepts_valid_real_and_imag_float_operations() {
    assert_ok(
        r#"
fn f_real(c complex128) float64 {
    return real(c)
}
fn f_imag(c complex128) float64 {
    return imag(c)
}
fn take_float(x float64) {}
fn main() {
    c := complex(1.0, 2.0)
    var r float64 = real(c)
    var im float64 = imag(c)
    r = real(c)
    im = imag(c)
    take_float(real(c))
    take_float(imag(c))
    _ = r + 1.5
    _ = im + 2.5
}
"#,
    );
}
