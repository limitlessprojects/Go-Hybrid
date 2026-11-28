//! Regression tests for #3137 (Rows 1-10):
//! Enforce OpKind::Enum typing for user enum values, constructor payload validation,
//! payload-binding types, foreign-variant match pattern rejection, enum-to-enum /
//! enum-to-int assignment rejection, invalid enum conversions, and field reads.

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

const ENUM_HEADER: &str = r#"
enum shape {
    circle(f64)
    rectangle(string)
    point
}
enum color {
    red
    green
}
"#;

#[test]
fn test_rows_1_to_3_constructor_payload_validation_rejected() {
    let cases = [
        (
            "row 1: payload string into f64",
            r#"
fn main() {
    s := shape.circle("x")
    _ = s
}
"#,
        ),
        (
            "row 2: payload float into string",
            r#"
fn main() {
    s := shape.rectangle(1.0)
    _ = s
}
"#,
        ),
        (
            "row 3: arity error (2 args to 1 payload)",
            r#"
fn main() {
    s := shape.rectangle("h", "i")
    _ = s
}
"#,
        ),
        (
            "row 3b: payload-free variant with arg",
            r#"
fn main() {
    s := shape.point(1)
    _ = s
}
"#,
        ),
        (
            "row 3c: payload variant with 0 args",
            r#"
fn main() {
    s := shape.circle()
    _ = s
}
"#,
        ),
    ];

    for (name, body) in cases {
        let src = format!("{}\n{}", ENUM_HEADER, body);
        let r = check(&src);
        assert_eq!(r.verdict, Verdict::Reject, "expected Reject for {name}");
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for {name}, got {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn test_rows_4_and_5_pattern_payload_bindings_rejected() {
    let cases = [
        (
            "row 4: r is f64 cannot assign to int",
            r#"
fn main() {
    s := shape.circle(2.0)
    match s {
        circle(r): {
            var x int = r
            _ = x
        }
        _: {}
    }
}
"#,
        ),
        (
            "row 5: r is f64 cannot add to string",
            r#"
fn main() {
    s := shape.circle(2.0)
    match s {
        circle(r): {
            _ = r + "x"
        }
        _: {}
    }
}
"#,
        ),
    ];

    for (name, body) in cases {
        let src = format!("{}\n{}", ENUM_HEADER, body);
        let r = check(&src);
        assert_eq!(r.verdict, Verdict::Reject, "expected Reject for {name}");
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for {name}, got {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn test_row_6_foreign_variant_pattern_rejected() {
    let cases = [
        (
            "row 6: bare foreign variant red on shape subject",
            r#"
fn main() {
    s := shape.point
    match s {
        red: {}
        _: {}
    }
}
"#,
        ),
        (
            "row 6b: qualified foreign variant color.red on shape subject",
            r#"
fn main() {
    s := shape.point
    match s {
        color.red: {}
        _: {}
    }
}
"#,
        ),
        (
            "unknown variant on shape subject",
            r#"
fn main() {
    s := shape.point
    match s {
        bogus: {}
        _: {}
    }
}
"#,
        ),
    ];

    for (name, body) in cases {
        let src = format!("{}\n{}", ENUM_HEADER, body);
        let r = check(&src);
        assert_eq!(r.verdict, Verdict::Reject, "expected Reject for {name}");
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for {name}, got {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn test_rows_7_to_10_assignments_conversions_and_field_reads_rejected() {
    let cases = [
        (
            "row 7: var c color = shape.point (enum into different enum)",
            r#"
fn main() {
    var c color = shape.point
    _ = c
}
"#,
        ),
        (
            "row 8: c := color.red; var n int = c (enum into int)",
            r#"
fn main() {
    c := color.red
    var n int = c
    _ = n
}
"#,
        ),
        (
            "row 9: c := color.red; _ = int(c) (enum conversion without constants)",
            r#"
fn main() {
    c := color.red
    _ = int(c)
}
"#,
        ),
        (
            "row 10: s := shape.circle(2.0); _ = s.radius (field read on enum)",
            r#"
fn main() {
    s := shape.circle(2.0)
    _ = s.radius
}
"#,
        ),
        (
            "enum comparison with int",
            r#"
fn main() {
    c := color.red
    if c == 1 {}
}
"#,
        ),
        (
            "enum comparison with different enum",
            r#"
fn main() {
    c := color.red
    if c == shape.point {}
}
"#,
        ),
        (
            "enum comparison with untyped nil",
            r#"
fn main() {
    c := color.red
    if c == nil {}
}
"#,
        ),
        (
            "enum ordered comparison",
            r#"
fn main() {
    c := color.red
    if c < color.green {}
}
"#,
        ),
    ];

    for (name, body) in cases {
        let src = format!("{}\n{}", ENUM_HEADER, body);
        let r = check(&src);
        assert_eq!(r.verdict, Verdict::Reject, "expected Reject for {name}");
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for {name}, got {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn test_valid_enum_operations_accepted() {
    let cases = [
        (
            "valid assignment and same enum copy",
            r#"
fn main() {
    s := shape.point
    var s2 shape = s
    _ = s2
    c := color.red
    var c2 color = c
    _ = c2
}
"#,
        ),
        (
            "valid pattern matching with payload bindings",
            r#"
fn main() {
    s := shape.rectangle("hello")
    match s {
        circle(r): {
            var f f64 = r
            _ = f
        }
        rectangle(a): {
            var str string = a
            _ = str
        }
        point: {}
    }
}
"#,
        ),
        (
            "valid enum equality comparison between same enum",
            r#"
fn main() {
    c1 := color.red
    c2 := color.green
    if c1 == c2 {
        _ = c1
    }
}
"#,
        ),
        (
            "valid assignment to any interface",
            r#"
fn main() {
    var a any = shape.point
    _ = a
}
"#,
        ),
        (
            "valid function parameter and return of enum type",
            r#"
fn pick(c color) color {
    return c
}
fn main() {
    c := pick(color.red)
    _ = c
}
"#,
        ),
    ];

    for (name, body) in cases {
        let src = format!("{}\n{}", ENUM_HEADER, body);
        let r = check(&src);
        assert_eq!(r.verdict, Verdict::Accept, "expected Accept for {name}, got {:?}", r.diagnostics);
        assert!(error_codes(&r).is_empty(), "expected no errors for {name}, got {:?}", r.diagnostics);
    }
}
