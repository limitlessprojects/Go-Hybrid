//! Regression tests for #2534:
//! match arm literal patterns conflicting with subject type must be rejected
//! with HYB-E1002, preventing broken Go code from escaping to go build.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> (Verdict, Vec<hy_diag::Diagnostic>) {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    (res.verdict, res.diagnostics)
}

#[test]
fn match_arm_literal_conflicting_types_rejected_e1002() {
    let cases = [
        // 1. String subject matching int literal
        r#"
fn main() {
    var s string = "hello"
    match s {
        1 => {}
        _ => {}
    }
}
"#,
        // 2. String subject matching bool literal
        r#"
fn main() {
    var s string = "hello"
    match s {
        true => {}
        _ => {}
    }
}
"#,
        // 3. Bool subject matching string literal
        r#"
fn main() {
    var b bool = true
    match b {
        "true" => {}
        _ => {}
    }
}
"#,
        // 4. Int subject matching string literal
        r#"
fn main() {
    var i int = 42
    match i {
        "42" => {}
        _ => {}
    }
}
"#,
        // 5. Int subject matching float literal
        r#"
fn main() {
    var i int = 42
    match i {
        3.14 => {}
        _ => {}
    }
}
"#,
    ];

    for (idx, src) in cases.iter().enumerate() {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Reject,
            "case #{idx} should reject, got Accept:\n{src}"
        );
        let has_e1002 = diags.iter().any(|d| d.code == "HYB-E1002");
        assert!(
            has_e1002,
            "case #{idx} should emit HYB-E1002, got: {diags:?}\n{src}"
        );
    }
}

#[test]
fn match_arm_literal_compatible_types_accepted() {
    let cases = [
        // 1. String subject matching string literal
        r#"
fn main() {
    var s string = "hello"
    match s {
        "hello" => {}
        "world" => {}
        _ => {}
    }
}
"#,
        // 2. Int subject matching int literal
        r#"
fn main() {
    var i int = 10
    match i {
        10 => {}
        20 => {}
        _ => {}
    }
}
"#,
        // 3. Float subject matching untyped int literal and float literal
        r#"
fn main() {
    var f float64 = 1.0
    match f {
        1 => {}
        2.5 => {}
        _ => {}
    }
}
"#,
        // 4. Bool subject matching bool literal with trailing wildcard
        r#"
fn main() {
    var b bool = true
    match b {
        true => {}
        false => {}
        _ => {}
    }
}
"#,
    ];

    for (idx, src) in cases.iter().enumerate() {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Accept,
            "compatible case #{idx} should accept, got Reject with diags: {diags:?}\n{src}"
        );
    }
}
