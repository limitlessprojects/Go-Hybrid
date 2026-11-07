//! issue_3131_append_spread_constraints.rs — tests for HYB-E1002 enforcement
//! on invalid variadic spread positions and counts in append() and built-ins (fixes #3131).

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
fn invalid_append_spread_positions_and_counts_reject_e1002() {
    let cases = [
        // 1. Spread on non-final argument in append
        r#"
fn main() {
    var a []int
    var b []int
    a = append(a, b..., 1)
    _ = a
}
"#,
        // 2. More than 2 arguments when using spread in append
        r#"
fn main() {
    var a []int
    var b []int
    a = append(a, 1, b...)
    _ = a
}
"#,
        // 3. Multiple spread arguments in append
        r#"
fn main() {
    var a []int
    var b []int
    var c []int
    a = append(a, b..., c...)
    _ = a
}
"#,
        // 4. Spread on single argument in append
        r#"
fn main() {
    var a []int
    a = append(a...)
    _ = a
}
"#,
        // 5. Spread on first argument with subsequent arguments
        r#"
fn main() {
    var a [][]int
    var b []int
    a = append(a..., b)
    _ = a
}
"#,
    ];

    for (i, src) in cases.iter().enumerate() {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "case {} should be rejected:\n{}",
            i + 1,
            src
        );
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "case {} missing HYB-E1002: {:?}",
            i + 1,
            r.diagnostics
        );
    }
}

#[test]
fn valid_append_spread_positive_controls_accepted() {
    let cases = [
        // 1. Valid 2-argument slice spread append
        r#"
fn main() {
    var a []int
    var b []int
    a = append(a, b...)
    _ = a
}
"#,
        // 2. Valid string spread append to byte slice
        r#"
fn main() {
    var bs []byte
    bs = append(bs, "hello"...)
    _ = bs
}
"#,
        // 3. Single element append
        r#"
fn main() {
    var a []int
    a = append(a, 1)
    _ = a
}
"#,
        // 4. Multiple discrete element append
        r#"
fn main() {
    var a []int
    a = append(a, 1, 2, 3)
    _ = a
}
"#,
        // 5. Single argument append (valid in Go)
        r#"
fn main() {
    var a []int
    a = append(a)
    _ = a
}
"#,
    ];

    for (i, src) in cases.iter().enumerate() {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "case {} should be accepted:\n{}",
            i + 1,
            src
        );
        assert_eq!(
            error_codes(&r).len(),
            0,
            "case {} should have no errors: {:?}",
            i + 1,
            r.diagnostics
        );
    }
}
