use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> (Verdict, Vec<hy_diag::Diagnostic>) {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    (res.verdict, res.diagnostics)
}

#[test]
fn binary_mixed_numeric_variables_rejected() {
    let cases = [
        // Arithmetic +
        r#"
fn main() {
    var f float64 = 1.0
    var i int = 2
    _ = f + i
}
"#,
        // Arithmetic -
        r#"
fn main() {
    var f float64 = 1.0
    var i int = 2
    _ = f - i
}
"#,
        // Arithmetic *
        r#"
fn main() {
    var f float64 = 1.0
    var i int = 2
    _ = f * i
}
"#,
        // Arithmetic /
        r#"
fn main() {
    var f float64 = 1.0
    var i int = 2
    _ = f / i
}
"#,
        // Reversed operands i + f
        r#"
fn main() {
    var f float64 = 1.0
    var i int = 2
    _ = i + f
}
"#,
        // Comparison <
        r#"
fn main() {
    var f float64 = 1.0
    var i int = 2
    _ = f < i
}
"#,
        // Equality ==
        r#"
fn main() {
    var f float64 = 1.0
    var i int = 2
    _ = (f == i)
}
"#,
        // Inequality !=
        r#"
fn main() {
    var f float64 = 1.0
    var i int = 2
    _ = (f != i)
}
"#,
    ];

    for src in cases {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Reject,
            "expected reject for binary operation on mismatched numeric types:\n{src}"
        );
        assert!(
            diags.iter().any(|d| d.code == "HYB-E1002"),
            "expected HYB-E1002 in diags: {:?}",
            diags
        );
    }
}

#[test]
fn binary_untyped_const_and_matching_types_accepted() {
    let src = r#"
const c = 2

fn main() {
    var f float64 = 1.0
    var f2 float64 = 2.0
    var i int = 3
    var i2 int = 4

    // Matching types
    _ = f + f2
    _ = f - f2
    _ = f * f2
    _ = f / f2
    _ = f < f2
    _ = (f == f2)

    _ = i + i2
    _ = i - i2
    _ = i * i2
    _ = i / i2
    _ = i < i2
    _ = (i == i2)

    // Untyped integer constants with float (both sides)
    _ = f + 1
    _ = 1 + f
    _ = f - 1
    _ = 1 - f
    _ = f * 2
    _ = 2 * f
    _ = f / 2
    _ = 2 / f
    _ = f < 2
    _ = 2 < f
    _ = (f == 2)
    _ = (2 == f)

    // Named untyped constants
    _ = f + c
    _ = c + f
    _ = f < c
    _ = (f == c)
}
"#;
    let (verdict, diags) = check(src);
    assert_eq!(
        verdict,
        Verdict::Accept,
        "expected accept for valid untyped constants and matching types: {:?}",
        diags
    );
}
