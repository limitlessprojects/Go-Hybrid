use hy_check::{check_file, CheckResult};
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect()
}

#[test]
fn test_multibind_len_rejected() {
    let src = r#"
package main

fn main() {
    xs := []int{1, 2}
    a, b := len(xs)
    _ = a
    _ = b
}
"#;
    let r = check(src);
    let diags = error_codes(&r);
    assert!(!diags.is_empty(), "expected HYB-E1002 for a, b := len(xs)");
    assert!(
        diags.iter().any(|d| d.contains("HYB-E1002") && d.contains("len") && d.contains("returns 1 value")),
        "unexpected diags: {:?}",
        diags
    );
}

#[test]
fn test_multibind_cap_rejected() {
    let src = r#"
package main

fn main() {
    xs := []int{1, 2}
    a, b := cap(xs)
    _ = a
    _ = b
}
"#;
    let r = check(src);
    let diags = error_codes(&r);
    assert!(!diags.is_empty(), "expected HYB-E1002 for a, b := cap(xs)");
    assert!(
        diags.iter().any(|d| d.contains("HYB-E1002") && d.contains("cap") && d.contains("returns 1 value")),
        "unexpected diags: {:?}",
        diags
    );
}

#[test]
fn test_multibind_make_rejected() {
    let src = r#"
package main

fn main() {
    a, b := make([]int, 5)
    _ = a
    _ = b
}
"#;
    let r = check(src);
    let diags = error_codes(&r);
    assert!(!diags.is_empty(), "expected HYB-E1002 for a, b := make([]int, 5)");
    assert!(
        diags.iter().any(|d| d.contains("HYB-E1002") && d.contains("make") && d.contains("returns 1 value")),
        "unexpected diags: {:?}",
        diags
    );
}

#[test]
fn test_multibind_new_rejected() {
    let src = r#"
package main

fn main() {
    a, b := new(int)
    _ = a
    _ = b
}
"#;
    let r = check(src);
    let diags = error_codes(&r);
    assert!(!diags.is_empty(), "expected HYB-E1002 for a, b := new(int)");
    assert!(
        diags.iter().any(|d| d.contains("HYB-E1002") && d.contains("new") && d.contains("returns 1 value")),
        "unexpected diags: {:?}",
        diags
    );
}

#[test]
fn test_multibind_copy_rejected() {
    let src = r#"
package main

fn main() {
    dst := []int{1, 2}
    src_slice := []int{3, 4}
    a, b := copy(dst, src_slice)
    _ = a
    _ = b
}
"#;
    let r = check(src);
    let diags = error_codes(&r);
    assert!(!diags.is_empty(), "expected HYB-E1002 for a, b := copy(...)");
    assert!(
        diags.iter().any(|d| d.contains("HYB-E1002") && d.contains("copy") && d.contains("returns 1 value")),
        "unexpected diags: {:?}",
        diags
    );
}

#[test]
fn test_multibind_min_max_rejected() {
    let src = r#"
package main

fn main() {
    a, b := min(1, 2)
    c, d := max(1, 2)
    _ = a
    _ = b
    _ = c
    _ = d
}
"#;
    let r = check(src);
    let diags = error_codes(&r);
    assert!(diags.len() >= 2, "expected HYB-E1002 for min and max multi-binds");
    assert!(
        diags.iter().any(|d| d.contains("HYB-E1002") && d.contains("min")),
        "unexpected diags: {:?}",
        diags
    );
    assert!(
        diags.iter().any(|d| d.contains("HYB-E1002") && d.contains("max")),
        "unexpected diags: {:?}",
        diags
    );
}

#[test]
fn test_multibind_complex_real_imag_rejected() {
    let src = r#"
package main

fn main() {
    a, b := complex(1.0, 2.0)
    c, d := real(complex(1.0, 2.0))
    e, f := imag(complex(1.0, 2.0))
    _ = a
    _ = b
    _ = c
    _ = d
    _ = e
    _ = f
}
"#;
    let r = check(src);
    let diags = error_codes(&r);
    assert!(diags.len() >= 3, "expected HYB-E1002 for complex, real, imag multi-binds");
}

#[test]
fn test_multibind_some_rejected() {
    let src = r#"
package main

fn main() {
    a, b := some(42)
    _ = a
    _ = b
}
"#;
    let r = check(src);
    let diags = error_codes(&r);
    assert!(!diags.is_empty(), "expected HYB-E1002 for a, b := some(42)");
    assert!(
        diags.iter().any(|d| d.contains("HYB-E1002") && d.contains("some") && d.contains("returns 1 value")),
        "unexpected diags: {:?}",
        diags
    );
}

#[test]
fn test_multibind_primitive_conversions_rejected() {
    let src = r#"
package main

fn main() {
    x := 42
    a, b := int32(x)
    c, d := float64(x)
    _ = a
    _ = b
    _ = c
    _ = d
}
"#;
    let r = check(src);
    let diags = error_codes(&r);
    assert!(diags.len() >= 2, "expected HYB-E1002 for type conversion multi-binds");
    assert!(
        diags.iter().any(|d| d.contains("HYB-E1002") && d.contains("int32")),
        "unexpected diags: {:?}",
        diags
    );
    assert!(
        diags.iter().any(|d| d.contains("HYB-E1002") && d.contains("float64")),
        "unexpected diags: {:?}",
        diags
    );
}

#[test]
fn test_multibind_type_alias_conversion_rejected() {
    let src = r#"
package main

type my_int = int

fn main() {
    x := 42
    a, b := my_int(x)
    _ = a
    _ = b
}
"#;
    let r = check(src);
    let diags = error_codes(&r);
    assert!(!diags.is_empty(), "expected HYB-E1002 for a, b := my_int(x)");
    assert!(
        diags.iter().any(|d| d.contains("HYB-E1002") && d.contains("my_int")),
        "unexpected diags: {:?}",
        diags
    );
}

#[test]
fn test_multibind_assignment_rejected() {
    let src = r#"
package main

fn main() {
    xs := []int{1, 2}
    var a int
    var b int
    a, b = len(xs)
    _ = a
    _ = b
}
"#;
    let r = check(src);
    let diags = error_codes(&r);
    assert!(!diags.is_empty(), "expected HYB-E1002 for a, b = len(xs)");
    assert!(
        diags.iter().any(|d| d.contains("HYB-E1002") && d.contains("len")),
        "unexpected diags: {:?}",
        diags
    );
}

#[test]
fn test_valid_single_value_and_multivalue_controls() {
    let src = r#"
package main

fn pair() (int, string) {
    return 1, "hello"
}

fn main() {
    xs := []int{1, 2}
    n := len(xs)
    c := cap(xs)
    m := make([]int, 5)
    p := new(int)
    s := some(42)
    val := int32(n)
    x, y := pair()
    _ = c
    _ = m
    _ = p
    _ = s
    _ = val
    _ = x
    _ = y
}
"#;
    let r = check(src);
    let diags = error_codes(&r);
    assert!(diags.is_empty(), "expected 0 diags for valid controls, got: {:?}", diags);
}
