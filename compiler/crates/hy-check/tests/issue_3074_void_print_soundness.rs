//! Regression test for Issue #3074:
//! `print()` and `println()` void return values used in value positions must be rejected (HYB-E1002).
//! Under GUARANTEES.md Row 13 (BUILDABLE), any program accepted by `hyb check`
//! must emit Go code that passes `go build`.

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("parses");
    check_file(&ast)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn assert_e1002(src: &str) {
    let cs = codes(src);
    assert!(
        !cs.is_empty() && cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(
        cs.is_empty(),
        "expected 0 diagnostics, got {cs:?} for:\n{src}"
    );
}

#[test]
fn test_rejects_println_in_variable_definition() {
    assert_e1002(
        r#"
fn main() {
    x := println("hello")
    _ = x
}
"#,
    );
}

#[test]
fn test_rejects_print_in_variable_definition() {
    assert_e1002(
        r#"
fn main() {
    x := print("hello")
    _ = x
}
"#,
    );
}

#[test]
fn test_rejects_println_in_assignment() {
    assert_e1002(
        r#"
fn main() {
    var x int
    x = println("hello")
    _ = x
}
"#,
    );
}

#[test]
fn test_rejects_println_in_return() {
    assert_e1002(
        r#"
fn f() int {
    return println(1)
}
fn main() {
    _ = f()
}
"#,
    );
}

#[test]
fn test_rejects_println_in_binary_op() {
    assert_e1002(
        r#"
fn main() {
    _ = println(1) + 1
}
"#,
    );
}

#[test]
fn test_rejects_println_in_call_arg() {
    assert_e1002(
        r#"
fn worker(x int) {
    _ = x
}
fn main() {
    worker(println(1))
}
"#,
    );
}

#[test]
fn test_rejects_println_in_channel_send() {
    assert_e1002(
        r#"
fn main() {
    ch := make(chan int, 1)
    ch <- println(1)
}
"#,
    );
}

#[test]
fn test_rejects_println_in_task_spawn() {
    assert_e1002(
        r#"
fn worker(x int) {
    _ = x
}
fn main() {
    task worker(println(1))
}
"#,
    );
}

#[test]
fn test_accepts_valid_standalone_print_and_println() {
    assert_ok(
        r#"
fn main() {
    print("hello ")
    println("world")
}
"#,
    );
}
