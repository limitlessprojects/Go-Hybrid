//! Regression tests for `check_try_operand` on associated functions, struct methods,
//! local closures / `fn`-typed bindings, and predeclared builtins (`HYB-E2011`, Row 13).

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn test_try_on_non_error_associated_fn_rejects_e2011() {
    let src = r#"
struct box {
    v int
}

fn box.make_box() box {
    return box{v: 1}
}

fn main() {
    b := box.make_box()?
    _ = b
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E2011".to_string()));
}

#[test]
fn test_try_on_non_error_and_option_struct_methods_rejects_e2011() {
    for src in [
        r#"
struct box { v int }
fn (b box) get() int { return b.v }
fn main() {
    b := box{v: 1}
    x := b.get()?
    _ = x
}
"#,
        r#"
struct box { v int }
fn (b box) opt() option[int] { return some(b.v) }
fn main() {
    b := box{v: 1}
    x := b.opt()?
    _ = x
}
"#,
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject);
        assert!(error_codes(&r).contains(&"HYB-E2011".to_string()));
    }
}

#[test]
fn test_try_on_local_closure_and_fn_param_without_error_rejects_e2011() {
    for src in [
        r#"
fn main() {
    f := fn() int { return 1 }
    x := f()?
    _ = x
}
"#,
        r#"
fn run(cb fn() int) error {
    x := cb()?
    _ = x
    return nil
}
fn main() {
    _ = run(fn() int { return 2 })
}
"#,
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject);
        assert!(error_codes(&r).contains(&"HYB-E2011".to_string()));
    }
}

#[test]
fn test_try_on_predeclared_builtin_rejects_e2011() {
    let src = r#"
fn main() {
    n := len("hello")?
    _ = n
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E2011".to_string()));
}

#[test]
fn test_try_on_valid_error_returning_assoc_method_and_closure_accepts() {
    let src = r#"
struct box {
    v int
}

fn box.try_make() (box, error) {
    return box{v: 1}, nil
}

fn (b box) try_get() (int, error) {
    return b.v, nil
}

fn run(cb fn() (int, error)) error {
    f := fn() (int, error) { return 1, nil }
    b := box.try_make()?
    x := b.try_get()?
    y := f()?
    z := cb()?
    _ = x + y + z
    return nil
}

fn main() {
    _ = run(fn() (int, error) { return 2, nil })
}
"#;
    let r = check(src);
    assert!(error_codes(&r).is_empty(), "got: {:?}", r.diagnostics);
    assert_eq!(r.verdict, Verdict::Accept);
}