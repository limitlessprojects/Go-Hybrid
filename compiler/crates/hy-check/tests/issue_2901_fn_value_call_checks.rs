//! issue_2901_fn_value_call_checks.rs — issue #2901 (Row 13 BUILDABLE).
//!
//! Calls through fn values from an index (`fs[0]("a")`, `m["a"](1)`),
//! struct field (`v.cb("a")`), or call result (`mk()("a")`, `mk()(1, 2)`,
//! `var s string = mk()(1)`) must check arity, argument types, and result kinds.

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
fn fn_val_call_invalid_cases_rejected_with_e1002() {
    let cases = [
        (
            "call result arg type mismatch",
            r#"
fn mk() fn(int) int {
    return fn(x int) int { return x }
}
fn main() {
    _ = mk()("a")
}
"#,
        ),
        (
            "call result arity mismatch",
            r#"
fn mk() fn(int) int {
    return fn(x int) int { return x }
}
fn main() {
    _ = mk()(1, 2)
}
"#,
        ),
        (
            "call result return kind mismatch",
            r#"
fn mk() fn(int) int {
    return fn(x int) int { return x }
}
fn main() {
    var s string = mk()(1)
    _ = s
}
"#,
        ),
        (
            "call result kind binary mismatch",
            r#"
fn mk() fn(int) int {
    return fn(x int) int { return x }
}
fn main() {
    x := mk()(1)
    _ = x + "a"
}
"#,
        ),
        (
            "slice indexed fn arg type mismatch",
            r#"
fn identity(x int) int { return x }
fn main() {
    fs := []fn(int) int{identity}
    _ = fs[0]("a")
}
"#,
        ),
        (
            "map indexed fn result kind mismatch",
            r#"
fn identity(x int) int { return x }
fn main() {
    m := map[string]fn(int) int{"a": identity}
    var s string = m["a"](1)
    _ = s
}
"#,
        ),
        (
            "struct field fn arg type mismatch",
            r#"
struct h {
    cb fn(int) int
}
fn identity(x int) int { return x }
fn main() {
    v := h{cb: identity}
    _ = v.cb("a")
}
"#,
        ),
        (
            "slice indexed fn arity mismatch",
            r#"
fn identity(x int) int { return x }
fn main() {
    fs := []fn(int) int{identity}
    _ = fs[0](1, 2)
}
"#,
        ),
        (
            "struct field fn arity mismatch",
            r#"
struct h {
    cb fn(int) int
}
fn identity(x int) int { return x }
fn main() {
    v := h{cb: identity}
    _ = v.cb(1, 2)
}
"#,
        ),
        (
            "slice indexed fn result kind mismatch",
            r#"
fn identity(x int) int { return x }
fn main() {
    fs := []fn(int) int{identity}
    var s string = fs[0](1)
    _ = s
}
"#,
        ),
    ];

    for (name, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "case '{name}' should be rejected"
        );
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "case '{name}' should emit HYB-E1002, got: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn fn_val_call_valid_controls_accepted() {
    let src = r#"
struct h {
    cb fn(int) int
}
fn identity(x int) int { return x }
fn mk() fn(int) int {
    return fn(x int) int { return x }
}
fn main() {
    v := h{cb: identity}
    _ = v.cb(1)

    fs := []fn(int) int{identity}
    _ = fs[0](2)

    m := map[string]fn(int) int{"a": identity}
    _ = m["a"](3)

    var n int = mk()(4)
    _ = n
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "valid fn-value calls should be accepted, got: {:?}",
        r.diagnostics
    );
}
