//! issue_2901_fn_values_calls.rs — regression tests for Issue #2901:
//! calls through fn values from index, struct field or call result
//! (fs[0]("a"), v.cb("a"), mk()(1, 2), var s string = m["a"](1))
//! check arity, argument types, and result kinds (HYB-E1002) under Row 13 (BUILDABLE).

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
fn test_call_result_callee_args_and_arity_rejected() {
    let cases = [
        (
            "argument type mismatch on call result callee",
            r#"
fn mk() fn(int) int {
    return fn(x int) int { return x }
}
fn test() {
    _ = mk()("a")
}
"#,
        ),
        (
            "too many arguments on call result callee",
            r#"
fn mk() fn(int) int {
    return fn(x int) int { return x }
}
fn test() {
    _ = mk()(1, 2)
}
"#,
        ),
        (
            "not enough arguments on call result callee",
            r#"
fn mk() fn(int) int {
    return fn(x int) int { return x }
}
fn test() {
    _ = mk()()
}
"#,
        ),
    ];

    for (desc, src) in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for {}", desc);
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for {}, got diagnostics: {:?}",
            desc,
            r.diagnostics
        );
    }
}

#[test]
fn test_call_result_return_type_checks_rejected() {
    let cases = [
        (
            "cannot use int call result as string in variable declaration",
            r#"
fn mk() fn(int) int {
    return fn(x int) int { return x }
}
fn test() {
    var s string = mk()(1)
    _ = s
}
"#,
        ),
        (
            "mismatched types int and string in addition",
            r#"
fn mk() fn(int) int {
    return fn(x int) int { return x }
}
fn test() {
    x := mk()(1)
    _ = x + "a"
}
"#,
        ),
    ];

    for (desc, src) in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for {}", desc);
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for {}, got diagnostics: {:?}",
            desc,
            r.diagnostics
        );
    }
}

#[test]
fn test_slice_and_map_index_fn_calls_rejected() {
    let cases = [
        (
            "slice index fn argument type mismatch",
            r#"
fn test() {
    fs := []fn(int) int{fn(x int) int { return x }}
    _ = fs[0]("a")
}
"#,
        ),
        (
            "slice index fn too many arguments",
            r#"
fn test() {
    fs := []fn(int) int{fn(x int) int { return x }}
    _ = fs[0](1, 2)
}
"#,
        ),
        (
            "map index fn return type mismatch in var decl",
            r#"
fn test() {
    m := map[string]fn(int) int{"a": fn(x int) int { return x }}
    var s string = m["a"](1)
    _ = s
}
"#,
        ),
        (
            "map index fn argument type mismatch",
            r#"
fn test() {
    m := map[string]fn(int) int{"a": fn(x int) int { return x }}
    _ = m["a"]("bad")
}
"#,
        ),
    ];

    for (desc, src) in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for {}", desc);
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for {}, got diagnostics: {:?}",
            desc,
            r.diagnostics
        );
    }
}

#[test]
fn test_struct_field_fn_calls_rejected() {
    let cases = [
        (
            "struct field fn argument type mismatch",
            r#"
struct h {
    cb fn(int) int
}
fn test() {
    v := h{cb: fn(x int) int { return x }}
    _ = v.cb("a")
}
"#,
        ),
        (
            "struct field fn arity mismatch",
            r#"
struct h {
    cb fn(int) int
}
fn test() {
    v := h{cb: fn(x int) int { return x }}
    _ = v.cb(1, 2)
}
"#,
        ),
        (
            "struct field fn return type mismatch",
            r#"
struct h {
    cb fn(int) int
}
fn test() {
    v := h{cb: fn(x int) int { return x }}
    var s string = v.cb(1)
    _ = s
}
"#,
        ),
    ];

    for (desc, src) in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for {}", desc);
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for {}, got diagnostics: {:?}",
            desc,
            r.diagnostics
        );
    }
}

#[test]
fn test_valid_fn_value_calls_accepted() {
    let cases = [
        (
            "call result callee valid call",
            r#"
fn mk() fn(int) int {
    return fn(x int) int { return x }
}
fn test() {
    var n int = mk()(42)
    _ = n
}
"#,
        ),
        (
            "slice index fn valid call",
            r#"
fn test() {
    fs := []fn(int) int{fn(x int) int { return x }}
    var n int = fs[0](42)
    _ = n
}
"#,
        ),
        (
            "map index fn valid call",
            r#"
fn test() {
    m := map[string]fn(int) int{"a": fn(x int) int { return x }}
    var n int = m["a"](42)
    _ = n
}
"#,
        ),
        (
            "struct field fn valid call",
            r#"
struct h {
    cb fn(int) int
}
fn test() {
    v := h{cb: fn(x int) int { return x }}
    var n int = v.cb(42)
    _ = n
}
"#,
        ),
    ];

    for (desc, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "expected accept for {}, got diagnostics: {:?}",
            desc,
            r.diagnostics
        );
    }
}
