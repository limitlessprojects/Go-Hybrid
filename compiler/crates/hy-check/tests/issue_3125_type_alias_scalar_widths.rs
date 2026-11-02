//! issue_3125_type_alias_scalar_widths.rs — tests for HYB-E1002 enforcement
//! on numeric type aliases escaping scalar width checks (fixes #3125).

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
fn type_alias_scalar_width_mismatches_reject_e1002() {
    let cases = [
        // 1. Source variable is a type alias
        r#"
type my_i8 = int8
fn main() {
    var a my_i8 = 1
    var b int64 = a
    _ = b
}
"#,
        // 2. Destination variable is a type alias
        r#"
type my_i8 = int8
fn main() {
    var a int64 = 1
    var b my_i8 = a
    _ = b
}
"#,
        // 3. Binary arithmetic operand mismatch
        r#"
type my_i8 = int8
fn main() {
    var a my_i8 = 1
    var b int64 = 2
    c := a + b
    _ = c
}
"#,
        // 4. Binary comparison operand mismatch
        r#"
type my_i8 = int8
fn main() {
    var a my_i8 = 1
    var b int64 = 2
    c := a == b
    _ = c
}
"#,
        // 5. Function argument passing mismatch
        r#"
type my_i8 = int8
fn take(x int64) {}
fn main() {
    var a my_i8 = 1
    take(a)
}
"#,
        // 6. Slice literal element mismatch
        r#"
type my_i8 = int8
fn main() {
    var a my_i8 = 1
    s := []int64{a}
    _ = s
}
"#,
        // 7. Map value literal mismatch
        r#"
type my_i8 = int8
fn main() {
    var a my_i8 = 1
    m := map[string]int64{"key": a}
    _ = m
}
"#,
        // 8. Append element mismatch
        r#"
type my_i8 = int8
fn main() {
    var a my_i8 = 1
    var s []int64
    s = append(s, a)
    _ = s
}
"#,
        // 9. Compound assignment mismatch
        r#"
type my_i8 = int8
fn main() {
    var b int64 = 1
    var a my_i8 = 2
    b += a
    _ = b
}
"#,
        // 10. Chained type aliases
        r#"
type my_i8 = int8
type user_id = my_i8
fn main() {
    var u user_id = 1
    var b int64 = u
    _ = b
}
"#,
        // 11. Type alias conversion call as value
        r#"
type my_i8 = int8
fn main() {
    var b int64 = my_i8(1)
    _ = b
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
fn type_alias_scalar_positive_controls_accepted() {
    let cases = [
        // 1. Untyped constant initializer
        r#"
type my_i8 = int8
fn main() {
    var a my_i8 = 1
    _ = a
}
"#,
        // 2. Chained alias initialized with untyped constant
        r#"
type my_i8 = int8
type user_id = my_i8
fn main() {
    var u user_id = 42
    _ = u
}
"#,
        // 3. Explicit conversion matching target type
        r#"
type my_i8 = int8
fn main() {
    var a my_i8 = 1
    var b int64 = int64(a)
    _ = b
}
"#,
        // 4. Same underlying scalar append
        r#"
type my_i8 = int8
fn main() {
    var a my_i8 = 1
    var s []my_i8
    s = append(s, a)
    _ = s
}
"#,
        // 5. Slice literal with matching alias element
        r#"
type my_i8 = int8
fn main() {
    var a my_i8 = 1
    s := []my_i8{a}
    _ = s
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
