//! option_methods_and_pop_2955.rs — issue #2955 (Row 13 BUILDABLE, HYB-E1002).
//!
//! Regression test for issue #2955:
//! 1. `.pop()` called on an `option[T]` or `fn` value must be rejected with HYB-E1002
//!    instead of emitting an unbuildable `hy_pop(&o)` call.
//! 2. Non-existent method calls on `option[T]` (e.g. `o.foo()`) must be rejected with HYB-E1002.
//! 3. Wrong arities on `unwrap()` (0 args), `is_some()` (0 args), `is_none()` (0 args),
//!    and `or()` (1 arg) must be rejected with HYB-E1002.
//! 4. Valid option method calls stay accepted with 0 diagnostics.

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
fn option_and_fn_pop_rejected() {
    let src = r#"
fn takes_fn(f fn()) {
    f.pop()
}

fn main() {
    o := some(5)
    o.pop()
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().filter(|c| *c == "HYB-E1002").count() >= 2,
        "expected at least two HYB-E1002 errors, got {codes:?}"
    );
}

#[test]
fn option_nonexistent_method_rejected() {
    let src = r#"
fn main() {
    o := some(5)
    o.foo()
    o.len()
    o.clear()
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().filter(|c| *c == "HYB-E1002").count() >= 3,
        "expected at least three HYB-E1002 errors, got {codes:?}"
    );
}

#[test]
fn option_method_arity_mismatch_rejected() {
    let src = r#"
fn main() {
    o := some(5)
    _ = o.unwrap(1)
    _ = o.is_some(1)
    _ = o.is_none(1)
    _ = o.or()
    _ = o.or(1, 2)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().filter(|c| *c == "HYB-E1002").count() >= 5,
        "expected at least five HYB-E1002 errors, got {codes:?}"
    );
}

#[test]
fn option_valid_methods_stay_accepted() {
    let src = r#"
import "fmt"

fn main() {
    o := some(5)
    u := o.unwrap()
    v := o.or(0)
    if o.is_some() {
        fmt.println(u)
    }
    if o.is_none() {
        fmt.println(v)
    }
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "valid option methods must stay accepted: {:?}",
        r.diagnostics
    );
}
