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
fn test_addr_of_assign_target_simple_rejected() {
    let src = r#"
fn main() {
    x := 1
    if true {
        &x = 2
    }
    _ = x
}
"#;
    let r = check(src);
    let diags = error_codes(&r);
    assert!(!diags.is_empty(), "expected HYB-E2010 error for &x = 2");
    assert!(
        diags.iter().any(|d| d.contains("HYB-E2010") && d.contains("an address-of expression is not an assignable place")),
        "unexpected diags: {:?}",
        diags
    );
}

#[test]
fn test_addr_of_assign_target_compound_add_rejected() {
    let src = r#"
fn main() {
    x := 10
    if true {
        &x += 5
    }
    _ = x
}
"#;
    let r = check(src);
    let diags = error_codes(&r);
    assert!(!diags.is_empty(), "expected HYB-E2010 error for &x += 5");
    assert!(
        diags.iter().any(|d| d.contains("HYB-E2010") && d.contains("an address-of expression is not an assignable place")),
        "unexpected diags: {:?}",
        diags
    );
}

#[test]
fn test_addr_of_assign_target_compound_mul_rejected() {
    let src = r#"
fn main() {
    x := 10
    if true {
        &x *= 2
    }
    _ = x
}
"#;
    let r = check(src);
    let diags = error_codes(&r);
    assert!(!diags.is_empty(), "expected HYB-E2010 error for &x *= 2");
    assert!(
        diags.iter().any(|d| d.contains("HYB-E2010") && d.contains("an address-of expression is not an assignable place")),
        "unexpected diags: {:?}",
        diags
    );
}

#[test]
fn test_addr_of_assign_target_inc_dec_rejected() {
    let src = r#"
fn main() {
    x := 10
    if true {
        &x++
    }
    _ = x
}
"#;
    let r = check(src);
    let diags = error_codes(&r);
    assert!(!diags.is_empty(), "expected HYB-E2010 error for &x++");
    assert!(
        diags.iter().any(|d| d.contains("HYB-E2010") && d.contains("an address-of expression is not an assignable place")),
        "unexpected diags: {:?}",
        diags
    );
}

#[test]
fn test_addr_of_field_assign_target_rejected() {
    let src = r#"
struct point {
    x int
    y int
}

fn main() {
    pt := point{x: 1, y: 2}
    if true {
        &pt.x = 42
    }
    _ = pt
}
"#;
    let r = check(src);
    let diags = error_codes(&r);
    assert!(!diags.is_empty(), "expected HYB-E2010 error for &pt.x = 42");
    assert!(
        diags.iter().any(|d| d.contains("HYB-E2010") && d.contains("an address-of expression is not an assignable place")),
        "unexpected diags: {:?}",
        diags
    );
}

#[test]
fn test_addr_of_index_assign_target_rejected() {
    let src = r#"
fn main() {
    arr := [2]int{1, 2}
    if true {
        &arr[0] = 10
    }
    _ = arr
}
"#;
    let r = check(src);
    let diags = error_codes(&r);
    assert!(!diags.is_empty(), "expected HYB-E2010 error for &arr[0] = 10");
    assert!(
        diags.iter().any(|d| d.contains("HYB-E2010") && d.contains("an address-of expression is not an assignable place")),
        "unexpected diags: {:?}",
        diags
    );
}

#[test]
fn test_addr_of_parallel_assign_target_rejected() {
    let src = r#"
fn main() {
    a := 1
    b := 2
    if true {
        &a, b = 10, 20
    }
    _ = a
    _ = b
}
"#;
    let r = check(src);
    let diags = error_codes(&r);
    assert!(!diags.is_empty(), "expected HYB-E2010 error for &a in parallel assign");
    assert!(
        diags.iter().any(|d| d.contains("HYB-E2010") && d.contains("an address-of expression is not an assignable place")),
        "unexpected diags: {:?}",
        diags
    );
}

#[test]
fn test_valid_assignments_accepted_control() {
    let src = r#"
struct point {
    x int
    y int
}

fn main() {
    x := 1
    x = 2
    x += 5
    x++
    pt := point{x: 1, y: 2}
    pt.x = 10
    arr := [2]int{1, 2}
    arr[0] = 5
    p := &x
    _ = p
    _ = pt
    _ = arr
}
"#;
    let r = check(src);
    let diags = error_codes(&r);
    assert!(diags.is_empty(), "expected 0 diags for valid assignments, got: {:?}", diags);
}
