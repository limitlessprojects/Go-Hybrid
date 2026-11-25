//! const_complex_1803.rs — regression for issue #1803.
//!
//! Go permits complex64/complex128 conversions and the complex, real and imag
//! builtins in constant expressions. `is_conversion_type_name` omitted the
//! complex types and `expr_is_constant_inner` only recognised `len`, so valid
//! declarations were rejected with HYB-E6003.

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

fn assert_accept(src: &str) {
    let found = codes(src);
    assert!(
        found.is_empty(),
        "expected acceptance, got {found:?} for:\n{src}"
    );
}

fn assert_reject(src: &str) {
    let found = codes(src);
    assert!(
        found.iter().any(|code| code == "HYB-E6003"),
        "expected HYB-E6003, got {found:?} for:\n{src}"
    );
}

#[test]
fn accepts_complex_type_conversions_in_const() {
    assert_accept(
        r#"const c1 = complex64(1.0)
const c2 = complex128(2.0)

fn main() {
    fmt.println(c1)
    fmt.println(c2)
}
"#,
    );
}

#[test]
fn accepts_complex_builtin_and_real_imag() {
    assert_accept(
        r#"const c3 = complex(1.0, 2.0)
const c4 = real(c3)
const c5 = imag(c3)

fn main() {
    fmt.println(c3)
    fmt.println(c4)
    fmt.println(c5)
}
"#,
    );
}

#[test]
fn rejects_complex_of_runtime_value() {
    assert_reject(
        r#"fn main() {
    x := 1.0
    const c = complex(x, 2.0)
    fmt.println(c)
}
"#,
    );
}
