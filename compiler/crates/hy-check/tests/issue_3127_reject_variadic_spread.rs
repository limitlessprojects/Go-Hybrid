//! Regression test for issue #3127:
//! [soundness] hy-check: variadic spread (...) on method expressions, fn-values, field fns, and fn literals passes check and fails go build (Row 13 BUILDABLE)
//!
//! Under Go specification (§Calls) and Row 13 (BUILDABLE):
//! Hybrid functions, methods, closures, and function values never declare variadic parameters.
//! Passing variadic spread `...` to any non-variadic callable must be rejected with HYB-E1002.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("snippet should parse");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

// 1. Method expression call with variadic spread
#[test]
fn test_method_expression_variadic_spread_rejected() {
    let src = r#"
package main
struct s {
    x int
}
fn (self s) get_x(extra int) int {
    return self.x + extra
}
fn main() {
    val := s{ x: 1 }
    xs := []int{1}
    _ = s.get_x(val, xs...)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 2. Local function-typed value call with variadic spread
#[test]
fn test_local_fn_value_variadic_spread_rejected() {
    let src = r#"
package main
fn main() {
    fn_val := fn(x int) {}
    xs := []int{1}
    fn_val(xs...)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 3. Struct function-typed field call with variadic spread
#[test]
fn test_struct_field_fn_variadic_spread_rejected() {
    let src = r#"
package main
struct s {
    f fn(x int)
}
fn main() {
    val := s{ f: fn(x int) {} }
    xs := []int{1}
    val.f(xs...)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 4. Anonymous function literal call with variadic spread
#[test]
fn test_anonymous_fn_lit_slice_param_spread_rejected() {
    let src = r#"
package main
fn main() {
    xs := []int{1}
    (fn(p []int) {})(xs...)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 5. Valid non-variadic calls and legal append spread must be accepted
#[test]
fn test_valid_calls_and_append_spread_accepted() {
    let src = r#"
package main
struct s {
    x int
    f fn(x int)
}
fn (self s) get_x(extra int) int {
    return self.x + extra
}
fn main() {
    val := s{ x: 1, f: fn(x int) {} }
    _ = s.get_x(val, 2)
    val.f(3)
    fn_val := fn(x int) {}
    fn_val(4)
    (fn(x int) {})(5)

    // Legal append with spread
    xs := []int{1}
    ys := append(xs, xs...)
    _ = ys
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert_eq!(error_codes(&r), Vec::<String>::new());
}
