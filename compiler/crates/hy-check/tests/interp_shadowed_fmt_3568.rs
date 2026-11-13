//! interp_shadowed_fmt_3568.rs: regression tests for #3568 (GUARANTEES Row 13 BUILDABLE).
//!
//! Under GUARANTEES.md Row 13 (BUILDABLE), programs accepted by `hy-check` must emit
//! Go code that compiles under `go build`.
//!
//! In `hy-emit` (`compiler/crates/hy-emit/src/expr.rs:565-582`), a string interpolation with an
//! expression part lowers directly to `fmt.Sprintf(...)` and registers `self.use_package("fmt")`.
//!
//! When `fmt` is shadowed by an in-scope parameter, local binding, loop variable, closure argument,
//! generic type parameter, receiver type argument, struct, or package-level declaration, downstream
//! Go compilation fails because `fmt.Sprintf` resolves to the shadowing entity instead of the standard
//! library `fmt` package.
//!
//! `hy-check` must reject string interpolations with expression parts when `fmt` is shadowed with `HYB-E1002`.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {:?}, got {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(cs.is_empty(), "must accept {:?}, got {:?}", src, cs);
    assert_eq!(r.verdict, Verdict::Accept, "verdict for {:?}", src);
}

#[test]
fn test_interp_with_param_fmt_rejects() {
    let src = r#"
fn show(fmt i32) string {
    return "v={fmt}"
}
"#;
    rejects(src);
}

#[test]
fn test_interp_with_local_fmt_rejects() {
    let src = r#"
fn show(x i32) string {
    fmt := 42
    return "v={x}:{fmt}"
}
"#;
    rejects(src);
}

#[test]
fn test_interp_with_package_const_fmt_rejects() {
    let src = r#"
const fmt = "shadowed"

fn show(x i32) string {
    return "v={x}"
}
"#;
    rejects(src);
}

#[test]
fn test_interp_with_package_struct_fmt_rejects() {
    let src = r#"
struct fmt {
    val int
}

fn show(x i32) string {
    return "v={x}"
}
"#;
    rejects(src);
}

#[test]
fn test_interp_with_type_param_fmt_rejects() {
    let src = r#"
fn show[fmt any](x fmt) string {
    return "v={x}"
}
"#;
    rejects(src);
}

#[test]
fn test_interp_with_receiver_type_param_fmt_rejects() {
    let src = r#"
struct box[t any] {
    val t
}

fn (b box[fmt]) show() string {
    return "v={b.val}"
}
"#;
    rejects(src);
}

#[test]
fn test_interp_with_closure_param_fmt_rejects() {
    let src = r#"
fn show(x i32) string {
    f := fn(fmt string) string {
        return "msg={fmt}"
    }
    return f("ok")
}
"#;
    rejects(src);
}

#[test]
fn test_interp_with_loop_var_fmt_rejects() {
    let src = r#"
fn show() string {
    for fmt in 0..5 {
        if fmt == 2 {
            return "found={fmt}"
        }
    }
    return "none"
}
"#;
    rejects(src);
}

#[test]
fn test_positive_control_unshadowed_interp_accepts() {
    let src = r#"
fn show(x i32) string {
    return "v={x}"
}
"#;
    accepts(src);
}

#[test]
fn test_positive_control_sibling_name_format_accepts() {
    let src = r#"
fn show(format string, x i32) string {
    return "{format}:{x}"
}
"#;
    accepts(src);
}
