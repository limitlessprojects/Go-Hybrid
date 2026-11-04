use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

const SHARED_ENUMS: &str = r#"
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
fn test_row_7_enum_to_different_enum_assignment_rejected() {
    let src = format!(
        r#"{}
fn main() {{
    var c color = shape.point
    _ = c
}}
"#,
        SHARED_ENUMS
    );
    let res = check(&src);
    assert_eq!(res.verdict, Verdict::Reject, "expected rejection: shape into color");
    let has_e1002 = res.diagnostics.iter().any(|d| d.code == "HYB-E1002" || d.message.contains("cannot use"));
    assert!(has_e1002, "expected HYB-E1002 diagnostic on mismatched enum assignment: {:?}", res.diagnostics);
}

#[test]
fn test_row_8_enum_to_int_assignment_rejected() {
    let src = format!(
        r#"{}
fn main() {{
    c := color.red
    var n int = c
    _ = n
}}
"#,
        SHARED_ENUMS
    );
    let res = check(&src);
    assert_eq!(res.verdict, Verdict::Reject, "expected rejection: enum into int");
    let has_err = res.diagnostics.iter().any(|d| d.code == "HYB-E1002" || d.message.contains("cannot use"));
    assert!(has_err, "expected HYB-E1002 diagnostic on enum assigned to int: {:?}", res.diagnostics);
}

#[test]
fn test_row_10_field_access_on_enum_rejected() {
    let src = format!(
        r#"{}
fn main() {{
    s := shape.point
    _ = s.radius
}}
"#,
        SHARED_ENUMS
    );
    let res = check(&src);
    assert_eq!(res.verdict, Verdict::Reject, "expected rejection: field access on enum value");
    let has_err = res.diagnostics.iter().any(|d| d.code == "HYB-E1002" || d.message.contains("field") || d.message.contains("enum"));
    assert!(has_err, "expected diagnostic on enum field access: {:?}", res.diagnostics);
}

#[test]
fn test_row_6_match_foreign_enum_variant_rejected() {
    let src = format!(
        r#"{}
fn main() {{
    s := shape.point
    match s {{
        red: {{
            fmt.println("red")
        }}
        _: {{
            fmt.println("other")
        }}
    }}
}}
"#,
        SHARED_ENUMS
    );
    let res = check(&src);
    assert_eq!(res.verdict, Verdict::Reject, "expected rejection: foreign variant in match pattern: {:?}", res.diagnostics);
    let has_err = res.diagnostics.iter().any(|d| d.code == "HYB-E1002" || d.message.contains("not a variant of enum"));
    assert!(has_err, "expected diagnostic on foreign variant: {:?}", res.diagnostics);
}

#[test]
fn test_row_9_int_conversion_on_non_constant_enum_rejected() {
    let src = format!(
        r#"{}
fn main() {{
    c := color.red
    fmt.println(int(c))
}}
"#,
        SHARED_ENUMS
    );
    let res = check(&src);
    assert_eq!(res.verdict, Verdict::Reject, "expected rejection: int(c) on non-constant enum: {:?}", res.diagnostics);
}

#[test]
fn test_valid_same_enum_assignment_accepted() {
    let src = format!(
        r#"{}
fn main() {{
    c := color.red
    var c2 color = c
    _ = c2
}}
"#,
        SHARED_ENUMS
    );
    let res = check(&src);
    assert_eq!(res.verdict, Verdict::Accept, "expected valid same-enum assignment to be accepted: {:?}", res.diagnostics);
}
