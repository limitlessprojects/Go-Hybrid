use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn assert_reject_code(src: &str, expected_code: &str) {
    let ast = parse_file("test.hy", src).expect("must parse");
    let res = check_file(&ast);
    assert_eq!(
        res.verdict,
        Verdict::Reject,
        "expected Reject, got Accept: {:?}",
        res.diagnostics
    );
    assert!(
        res.diagnostics.iter().any(|d| d.code == expected_code),
        "expected diagnostic code {}, got: {:?}",
        expected_code,
        res.diagnostics
    );
}

fn assert_accept(src: &str) {
    let ast = parse_file("test.hy", src).expect("must parse");
    let res = check_file(&ast);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "expected Accept, got diagnostics: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_bare_builtins_evaluated_but_not_used() {
    // len
    assert_reject_code(
        r#"package main
fn main() {
    s := "abc"
    len(s)
}"#,
        "HYB-E1002",
    );

    // cap
    assert_reject_code(
        r#"package main
fn main() {
    s := []int{1}
    cap(s)
}"#,
        "HYB-E1002",
    );

    // make
    assert_reject_code(
        r#"package main
fn main() {
    make([]int, 5)
}"#,
        "HYB-E1002",
    );

    // new
    assert_reject_code(
        r#"package main
fn main() {
    new(int)
}"#,
        "HYB-E1002",
    );

    // append
    assert_reject_code(
        r#"package main
fn main() {
    s := []int{1}
    append(s, 2)
}"#,
        "HYB-E1002",
    );
}

#[test]
fn test_bare_type_conversions_evaluated_but_not_used() {
    assert_reject_code(
        r#"package main
fn main() {
    x := 1
    int(x)
}"#,
        "HYB-E1002",
    );

    assert_reject_code(
        r#"package main
fn main() {
    s := "abc"
    string(s)
}"#,
        "HYB-E1002",
    );
}

#[test]
fn test_bare_literals_variables_and_operations() {
    assert_reject_code(
        r#"package main
fn main() {
    x := 1
    x
}"#,
        "HYB-E1002",
    );

    assert_reject_code(
        r#"package main
fn main() {
    42
}"#,
        "HYB-E1002",
    );

    assert_reject_code(
        r#"package main
fn main() {
    1 + 2
}"#,
        "HYB-E1002",
    );

    assert_reject_code(
        r#"package main
fn main() {
    s := []int{1, 2}
    s[0]
}"#,
        "HYB-E1002",
    );
}

#[test]
fn test_defer_value_only_builtins_rejected() {
    assert_reject_code(
        r#"package main
fn main() {
    s := "abc"
    defer len(s)
}"#,
        "HYB-E2001",
    );

    assert_reject_code(
        r#"package main
fn main() {
    s := []int{1}
    defer cap(s)
}"#,
        "HYB-E2001",
    );

    assert_reject_code(
        r#"package main
fn main() {
    defer make([]int, 5)
}"#,
        "HYB-E2001",
    );

    assert_reject_code(
        r#"package main
fn main() {
    defer new(int)
}"#,
        "HYB-E2001",
    );

    assert_reject_code(
        r#"package main
fn main() {
    s := []int{1}
    defer append(s, 2)
}"#,
        "HYB-E2001",
    );
}

#[test]
fn test_defer_type_conversions_rejected() {
    assert_reject_code(
        r#"package main
fn main() {
    x := 1
    defer int(x)
}"#,
        "HYB-E2001",
    );
}

#[test]
fn test_valid_expression_statements_and_defers_accepted() {
    assert_accept(
        r#"package main

struct resource {}
fn (r resource) close() {}

fn cleanup() {}

fn main() {
    cleanup()
    r := resource{}
    r.close()
    ch := make(chan int, 1)
    ch <- 42
    <-ch
    defer cleanup()
    defer r.close()
    defer close(ch)
}
"#,
    );
}
