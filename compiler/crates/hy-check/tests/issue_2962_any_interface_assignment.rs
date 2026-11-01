//! Regression tests for Issue #2962:
//! Values of type `any` (or `recover()`) cannot be assigned to concrete types
//! in variable declarations, assignments, arguments, returns, or composite literals
//! without a type assertion (Row 13 BUILDABLE, HYB-E1002).

use hy_check::check_file;
use hy_syntax::parse_file;

fn error_codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("parses");
    hy_check::check_file(&ast)
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code)
        .collect()
}

fn assert_rejected(src: &str, expected_code: &str) {
    let codes = error_codes(src);
    assert!(
        codes.iter().any(|c| c == expected_code),
        "expected error code {expected_code}, got diagnostics {codes:?} for snippet:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let codes = error_codes(src);
    assert!(
        codes.is_empty(),
        "expected 0 errors, got {codes:?} for snippet:\n{src}"
    );
}

#[test]
fn test_any_variable_to_concrete_var_init_rejected() {
    let src = r#"
fn main() {
    var a any = 1
    var n int = a
    fmt.println(n)
}
"#;
    assert_rejected(src, "HYB-E1002");
}

#[test]
fn test_recover_to_concrete_call_arg_rejected() {
    let src = r#"
fn take(s string) {
    fmt.println(s)
}
fn main() {
    defer fn() {
        take(recover())
    }()
    panic("boom")
}
"#;
    assert_rejected(src, "HYB-E1002");
}

#[test]
fn test_recover_to_concrete_return_rejected() {
    let src = r#"
fn get() string {
    return recover()
}
fn main() {
    fmt.println(get())
}
"#;
    assert_rejected(src, "HYB-E1002");
}

#[test]
fn test_recover_in_slice_literal_rejected() {
    let src = r#"
fn main() {
    defer fn() {
        r := recover()
        xs := []string{r}
        fmt.println(xs)
    }()
    panic("boom")
}
"#;
    assert_rejected(src, "HYB-E1002");
}

#[test]
fn test_recover_to_concrete_var_init_rejected() {
    let src = r#"
fn main() {
    r := recover()
    var n int = r
    fmt.println(n)
}
"#;
    assert_rejected(src, "HYB-E1002");
}

#[test]
fn test_any_variable_to_concrete_assignment_rejected() {
    let src = r#"
fn main() {
    var a any = 1
    var n int = 0
    n = a
    fmt.println(n)
}
"#;
    assert_rejected(src, "HYB-E1002");
}

#[test]
fn test_any_interface_positive_controls_stay_accepted() {
    // 1. Any target accepts concrete values
    assert_ok(r#"
fn main() {
    var a any = 1
    a = "str"
    fmt.println("{a}")
}
"#);

    // 2. Returning concrete value to any result
    assert_ok(r#"
fn box() any {
    var g int = 1
    return g
}
fn main() {
    x := box()
    fmt.println("{x}")
}
"#);

    // 3. Passing to any param
    assert_ok(r#"
fn take(v any) {
    fmt.println("ok")
}
fn main() {
    take(1)
    take("hello")
}
"#);

    // 4. Interface in []any and map[string]any
    assert_ok(r#"
fn main() {
    xs := []any{1, "two"}
    m := map[string]any{}
    m["k"] = 1
    fmt.println("{len(xs)}")
}
"#);

    // 5. Equality comparison on any
    assert_ok(r#"
fn main() {
    var a any = 1
    if a == 1 {
        fmt.println("eq")
    }
}
"#);
}
