//! field_selector_void_soundness.rs — tests for HYB-E1002 rejection of
//! void calls and 0-value builtins as bases for field or method selections (`expr.field`).

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

// -----------------------------------------------------------------------------
// Negative: void calls and 0-value builtins in field or method selection
// -----------------------------------------------------------------------------

#[test]
fn rejects_void_call_field_selection() {
    let src = "
fn noop() {}

fn main() {
    x := noop().name
    _ = x
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn rejects_close_builtin_field_selection() {
    let src = "
fn main() {
    ch := make(chan int, 1)
    x := close(ch).name
    _ = x
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn rejects_void_call_method_call_receiver() {
    let src = "
fn noop() {}

fn main() {
    noop().run()
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn rejects_panic_method_call_receiver() {
    let src = "
fn main() {
    panic(\"abort\").run()
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn rejects_multi_value_method_call_receiver() {
    let src = "
fn two() (int, int) {
    return 1, 2
}

fn main() {
    two().run()
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// -----------------------------------------------------------------------------
// Positive Controls
// -----------------------------------------------------------------------------

#[test]
fn accepts_valid_field_and_method_selections() {
    let src = "
struct point {
    x int
    y int
}

fn (p *point) sum() int {
    return p.x + p.y
}

fn get_point() point {
    return point{x: 1, y: 2}
}

fn main() {
    pt := get_point()
    x := pt.x
    s := pt.sum()
    _ = x
    _ = s
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "valid field/method selections must pass");
}
