//! match_subject_and_pattern_soundness.rs — tests for HYB-E1002 rejection of
//! void calls and 0-value builtins in match subjects, and pattern literal type mismatches.

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
// Match Subject Void Rejections
// -----------------------------------------------------------------------------

#[test]
fn rejects_void_call_in_match_subject() {
    let src = "
fn noop() {}

fn main() {
    match noop() {
        0 => {}
        _ => {}
    }
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn rejects_close_builtin_in_match_subject() {
    let src = "
fn main() {
    ch := make(chan int, 1)
    match close(ch) {
        0 => {}
        _ => {}
    }
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn rejects_panic_builtin_in_match_subject() {
    let src = "
fn main() {
    match panic(\"abort\") {
        0 => {}
        _ => {}
    }
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// -----------------------------------------------------------------------------
// Match Arm Pattern Literal Type Mismatches
// -----------------------------------------------------------------------------

#[test]
fn rejects_int_pattern_on_string_subject() {
    let src = "
fn main() {
    s := \"hello\"
    match s {
        123 => {}
        _ => {}
    }
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn rejects_string_pattern_on_int_subject() {
    let src = "
fn main() {
    n := 42
    match n {
        \"not_an_int\" => {}
        _ => {}
    }
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn rejects_bool_pattern_on_int_subject() {
    let src = "
fn main() {
    n := 42
    match n {
        true => {}
        _ => {}
    }
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
fn accepts_valid_match_statements() {
    let src = "
fn main() {
    x := 42
    match x {
        0 => {}
        1 => {}
        _ => {}
    }

    s := \"hello\"
    match s {
        \"hello\" => {}
        \"world\" => {}
        _ => {}
    }

    b := true
    match b {
        true => {}
        _ => {}
    }
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "valid match statements must pass");
}
