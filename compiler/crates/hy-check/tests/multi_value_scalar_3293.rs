//! Regression pin for #3293: a multi-value call argument must match each
//! parameter's exact scalar width, not just its `OpKind`.

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn error_codes(src: &str) -> Vec<String> {
    let ast = parse_file("multi_3293.hy", src)
        .unwrap_or_else(|d| panic!("parse failed: {:?}", d));
    check_file(&ast)
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code)
        .collect()
}

/// `take(pair())` where pair returns int32 and take wants int64.
#[test]
fn multi_value_arg_with_mismatched_width_is_rejected() {
    let got = error_codes(
        r#"fn pair() (int32, int32) {
    return 1, 2
}

fn take(a int64, b int64) {
    _ = a
    _ = b
}

fn main() {
    take(pair())
}
"#,
    );
    assert!(
        got.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {got:?}"
    );
}

/// A float32 result into a float64 parameter.
#[test]
fn multi_value_arg_float_width_is_rejected() {
    let got = error_codes(
        r#"fn pair() (float32, float32) {
    return 1.0, 2.0
}

fn take(a float64, b float64) {
    _ = a
    _ = b
}

fn main() {
    take(pair())
}
"#,
    );
    assert!(
        got.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {got:?}"
    );
}

/// Negative control: matching widths stay accepted.
#[test]
fn multi_value_arg_with_matching_widths_stays_accepted() {
    let got = error_codes(
        r#"fn pair() (int64, int64) {
    return 1, 2
}

fn take(a int64, b int64) {
    _ = a
    _ = b
}

fn main() {
    take(pair())
}
"#,
    );
    assert!(got.is_empty(), "expected no errors, got {got:?}");
}