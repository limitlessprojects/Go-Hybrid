//! issue_3136_index_field_scalar_widths.rs — tests for HYB-E1002 enforcement
//! on typed scalar widths from slice/array indexing and struct field reads (fixes #3136).

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
fn index_and_field_scalar_width_mismatches_reject_e1002() {
    let cases = [
        // 1. Slice element indexing assigned to mismatched scalar
        r#"
fn main() {
    var s []int8
    var b int64 = s[0]
    _ = b
}
"#,
        // 2. Array element indexing assigned to mismatched scalar
        r#"
fn main() {
    var a [4]int8
    var b int64 = a[0]
    _ = b
}
"#,
        // 3. Struct field read assigned to mismatched scalar
        r#"
struct point {
    x int8
}
fn main() {
    var p point
    var b int64 = p.x
    _ = b
}
"#,
        // 4. Struct slice field indexing assigned to mismatched scalar
        r#"
struct bag {
    nums []int8
}
fn main() {
    var g bag
    var b int64 = g.nums[0]
    _ = b
}
"#,
        // 5. Slice element in binary arithmetic with mismatched operand
        r#"
fn main() {
    var s []int8
    var b int64 = 1
    c := s[0] + b
    _ = c
}
"#,
        // 6. Struct field in binary arithmetic with mismatched operand
        r#"
struct point {
    x int8
}
fn main() {
    var p point
    var b int64 = 1
    c := p.x + b
    _ = c
}
"#,
        // 7. Function argument passing with indexed value
        r#"
fn take(x int64) {}
fn main() {
    var s []int8
    take(s[0])
}
"#,
        // 8. Function argument passing with struct field
        r#"
fn take(x int64) {}
struct point {
    x int8
}
fn main() {
    var p point
    take(p.x)
}
"#,
        // 9. Slice literal with mismatched struct field element
        r#"
struct point {
    x int8
}
fn main() {
    var p point
    s := []int64{p.x}
    _ = s
}
"#,
        // 10. Append with mismatched indexed element
        r#"
fn main() {
    var s []int8
    var dst []int64
    dst = append(dst, s[0])
    _ = dst
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
fn index_and_field_scalar_positive_controls_accepted() {
    let cases = [
        // 1. Matching scalar slice index assignment
        r#"
fn main() {
    var s []int8
    var b int8 = s[0]
    _ = b
}
"#,
        // 2. Matching scalar struct field assignment
        r#"
struct point {
    x int8
}
fn main() {
    var p point
    var b int8 = p.x
    _ = b
}
"#,
        // 3. Matching scalar array index assignment
        r#"
fn main() {
    var a [4]int8
    var b int8 = a[0]
    _ = b
}
"#,
        // 4. Explicit conversion matching target type
        r#"
fn main() {
    var s []int8
    var b int64 = int64(s[0])
    _ = b
}
"#,
        // 5. Short variable definition inferred and used with matching type
        r#"
fn main() {
    var s []int8
    x := s[0]
    var b int8 = x
    _ = b
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
