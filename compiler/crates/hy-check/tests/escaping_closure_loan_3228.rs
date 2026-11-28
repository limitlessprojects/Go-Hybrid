//! Regression pin for #3228: a slot loan captured by a closure that
//! escapes through `return` must be rejected, not silently miscompiled.
//!
//! The loan lowers to copy-out now and store-back when its window ends. A
//! closure returned to the caller runs after that store-back, so every write
//! it makes through the loan lands on a dead copy. No emit-time placement
//! fixes it, so the checker has to refuse the shape.

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn error_codes(src: &str) -> Vec<String> {
    let ast = parse_file("escaping_closure_loan_3228.hy", src)
        .unwrap_or_else(|d| panic!("parse failed: {:?}", d));
    check_file(&ast)
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code)
        .collect()
}

/// The reported shape: return a closure that writes through a slice-element
/// loan.
#[test]
fn returned_closure_over_slice_loan_is_rejected() {
    let got = error_codes(
        r#"struct cell { n int }

fn mk(xs []cell) fn() {
    p := &xs[0]
    return fn() {
        p.n = 7
    }
}
"#,
    );
    assert!(
        got.iter().any(|c| c == "HYB-E3104"),
        "expected HYB-E3104, got {got:?}"
    );
}

/// The map-slot variant from the same report.
#[test]
fn returned_closure_over_map_slot_loan_is_rejected() {
    let got = error_codes(
        r#"struct cell { n int }

fn mkm(m map[string]cell) fn() {
    p := &m["a"]
    return fn() {
        p.n = 9
    }
}
"#,
    );
    assert!(
        got.iter().any(|c| c == "HYB-E3104"),
        "expected HYB-E3104, got {got:?}"
    );
}

/// Negative control: a closure capturing a plain local is fine. No loan is
/// involved, so there is no store-back to lose.
#[test]
fn returned_closure_over_plain_local_stays_accepted() {
    let got = error_codes(
        r#"fn maker() fn() {
    k := 3
    return fn() { fmt.println("k {k}") }
}
"#,
    );
    assert!(got.is_empty(), "expected no errors, got {got:?}");
}

/// Negative control: a closure that captures nothing is fine.
#[test]
fn returned_closure_with_no_captures_stays_accepted() {
    let got = error_codes(
        r#"fn plain() fn() {
    return fn() { fmt.println("plain") }
}
"#,
    );
    assert!(got.is_empty(), "expected no errors, got {got:?}");
}

/// Negative control: a loan written directly, without a closure, keeps
/// working. The fix must not turn ordinary loans into errors.
#[test]
fn direct_loan_write_stays_accepted() {
    let got = error_codes(
        r#"struct cell { n int }

fn main() {
    xs := []cell{cell{n: 1}}
    p := &xs[0]
    p.n = 5
    fmt.println(xs[0].n)
}
"#,
    );
    assert!(got.is_empty(), "expected no errors, got {got:?}");
}
