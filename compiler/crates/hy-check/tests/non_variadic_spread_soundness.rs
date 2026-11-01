//! non_variadic_spread_soundness.rs — regression tests for Issue #2701:
//! validation that variadic spread `...` is rejected on non-variadic functions,
//! methods, and non-variadic built-in functions under Row 13 BUILDABLE.

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
fn non_variadic_user_fn_with_spread_rejected() {
    // 1. Single-param function taking a slice cannot be called with spread
    let src1 = r#"
fn foo(s []int) {}
fn main() {
    xs := []int{1, 2}
    foo(xs...)
}
"#;
    let r1 = check(src1);
    assert_eq!(r1.verdict, Verdict::Reject);
    assert!(error_codes(&r1).contains(&"HYB-E1002".to_string()));

    // 2. Multi-param function cannot be called with spread
    let src2 = r#"
fn add(a int, b int) int { return a + b }
fn main() {
    xs := []int{1, 2}
    _ = add(xs...)
}
"#;
    let r2 = check(src2);
    assert_eq!(r2.verdict, Verdict::Reject);
    assert!(error_codes(&r2).contains(&"HYB-E1002".to_string()));
}

#[test]
fn non_variadic_method_with_spread_rejected() {
    let src = r#"
struct greeter {
    prefix string
}
fn (g greeter) greet(s []string) {}
fn main() {
    g := greeter{prefix: "Hi"}
    words := []string{"Alice"}
    g.greet(words...)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn non_variadic_builtins_with_spread_rejected() {
    // 1. len with spread
    let src_len = r#"
fn main() {
    xs := []int{1, 2}
    _ = len(xs...)
}
"#;
    let r_len = check(src_len);
    assert_eq!(r_len.verdict, Verdict::Reject);
    assert!(error_codes(&r_len).contains(&"HYB-E1002".to_string()));

    // 2. cap with spread
    let src_cap = r#"
fn main() {
    xs := []int{1, 2}
    _ = cap(xs...)
}
"#;
    let r_cap = check(src_cap);
    assert_eq!(r_cap.verdict, Verdict::Reject);
    assert!(error_codes(&r_cap).contains(&"HYB-E1002".to_string()));

    // 3. close with spread
    let src_close = r#"
fn main() {
    ch := make(chan int, 1)
    close(ch...)
}
"#;
    let r_close = check(src_close);
    assert_eq!(r_close.verdict, Verdict::Reject);
    assert!(error_codes(&r_close).contains(&"HYB-E1002".to_string()));

    // 4. delete with spread
    let src_del = r#"
fn main() {
    m := map[string]int{"a": 1}
    delete(m, "a"...)
}
"#;
    let r_del = check(src_del);
    assert_eq!(r_del.verdict, Verdict::Reject);
    assert!(error_codes(&r_del).contains(&"HYB-E1002".to_string()));
}

#[test]
fn task_spawn_non_variadic_with_spread_rejected() {
    let src = r#"
fn worker(s []int) {}
fn main() {
    xs := []int{1, 2}
    task worker(xs...)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn valid_variadic_append_and_plain_calls_accepted() {
    // 1. append with valid final spread argument
    let src_app = r#"
fn main() {
    s1 := []int{1}
    s2 := []int{2, 3}
    _ = append(s1, s2...)
}
"#;
    let r_app = check(src_app);
    assert_eq!(r_app.verdict, Verdict::Accept);

    // 2. normal function call without spread
    let src_normal = r#"
fn process(s []int) int { return len(s) }
fn main() {
    xs := []int{1, 2}
    _ = process(xs)
}
"#;
    let r_normal = check(src_normal);
    assert_eq!(r_normal.verdict, Verdict::Accept);
}
