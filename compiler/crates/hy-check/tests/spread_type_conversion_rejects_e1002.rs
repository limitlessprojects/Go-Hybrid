//! spread_type_conversion_rejects_e1002.rs — Row 13 (BUILDABLE).
//!
//! A type conversion `T(x)` takes exactly one expression without variadic spread `...`.
//! Passing `...` to a type conversion (`int(xs...)`, `string(bs...)`, `uint64(xs...)`, or `MyType(xs...)`)
//! emits unbuildable Go code (`cannot use ... in type conversion to <type>`).
//! `hy-check`'s `check_conversion_arity` must reject with `HYB-E1002`.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn spread_on_primitive_type_conversion_rejects_e1002() {
    let src = r#"
fn main() {
    xs := [1, 2]
    v := int(xs...)
    fmt.println("{v}")
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for int(xs...), got {:?}",
        r.diagnostics
    );
}

#[test]
fn spread_on_string_conversion_rejects_e1002() {
    let src = r#"
fn main() {
    bs := []byte{65, 66}
    s := string(bs...)
    fmt.println("{s}")
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for string(bs...), got {:?}",
        r.diagnostics
    );
}

#[test]
fn normal_type_conversion_accepted() {
    let src = r#"
fn main() {
    x := 42
    v := int64(x)
    fmt.println("{v}")
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
}
