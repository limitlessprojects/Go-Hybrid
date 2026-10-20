//! range_bound_and_interp_void_soundness.rs — tests for HYB-E1002 rejection of
//! void calls and zero-value builtins in integer range bounds (`for i in lo..hi`)
//! and string interpolation expressions (`"{expr}"`).

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
// Negative: void function calls in range bounds
// -----------------------------------------------------------------------------

#[test]
fn rejects_void_call_in_range_lo() {
    let src = "
fn noop() {}
fn main() {
    for i in noop()..10 {
        _ = i
    }
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn rejects_void_call_in_range_hi() {
    let src = "
fn noop() {}
fn main() {
    for i in 0..noop() {
        _ = i
    }
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn rejects_void_call_in_inclusive_range_hi() {
    let src = "
fn noop() {}
fn main() {
    for i in 0..=noop() {
        _ = i
    }
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// -----------------------------------------------------------------------------
// Negative: 0-value builtins in range bounds
// -----------------------------------------------------------------------------

#[test]
fn rejects_panic_in_range_lo() {
    let src = "
fn main() {
    for i in panic(\"boom\")..10 {
        _ = i
    }
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn rejects_panic_in_range_hi() {
    let src = "
fn main() {
    for i in 0..panic(\"boom\") {
        _ = i
    }
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn rejects_close_builtin_in_range_bounds() {
    let src = "
fn main() {
    ch := make(chan int, 1)
    for i in 0..close(ch) {
        _ = i
    }
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// -----------------------------------------------------------------------------
// Negative: void calls and 0-value builtins in string interpolation
// -----------------------------------------------------------------------------

#[test]
fn rejects_void_call_in_string_interpolation() {
    let src = "
fn noop() {}
fn main() {
    msg := \"hello {noop()}\"
    _ = msg
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn rejects_panic_in_string_interpolation() {
    let src = "
fn main() {
    reason := \"abort\"
    msg := \"status: {panic(reason)}\"
    _ = msg
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn rejects_close_in_string_interpolation() {
    let src = "
fn main() {
    ch := make(chan int, 1)
    msg := \"closed: {close(ch)}\"
    _ = msg
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
fn accepts_valid_integer_range_loops() {
    let src = "
fn get_limit() int {
    return 10
}

fn main() {
    sum := 0
    for i in 0..10 {
        sum = sum + i
    }
    for j in 1..=get_limit() {
        sum = sum + j
    }
    _ = sum
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "valid integer range loops must pass");
}

#[test]
fn accepts_valid_string_interpolation() {
    let src = "
fn get_name() string {
    return \"Go-Hybrid\"
}

fn main() {
    count := 42
    msg := \"welcome to {get_name()}, score is {count}!\"
    _ = msg
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "valid string interpolation must pass");
}
