//! Test suite for Issue #3064: error kind checking in returns, call arguments,
//! variables, and operations.
//!
//! Row 13 (BUILDABLE) / SPEC §3.1.

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

fn assert_code(src: &str, want: &str) {
    let cs = codes(src);
    assert!(
        cs.iter().any(|c| c == want),
        "expected [{want}] in {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let ast = parse_file("test.hy", src).expect("parses");
    let res = check_file(&ast);
    let msgs: Vec<_> = res.diagnostics.iter().map(|d| format!("{}: {}", d.code, d.message)).collect();
    assert!(res.diagnostics.is_empty(), "expected 0 diagnostics, got {msgs:?} for:\n{src}");
}

#[test]
fn test_error_return_scalar_rejected() {
    assert_code(
        r#"
package main

fn f() int {
    var err error
    return err
}
"#,
        "HYB-E1002",
    );

    assert_code(
        r#"
package main

fn f() string {
    var err error
    return err
}
"#,
        "HYB-E1002",
    );

    assert_code(
        r#"
package main

fn f() bool {
    var err error
    return err
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn test_scalar_return_in_error_fn_rejected() {
    assert_code(
        r#"
package main

fn f() error {
    return 42
}
"#,
        "HYB-E1002",
    );

    assert_code(
        r#"
package main

fn f() error {
    return "bad"
}
"#,
        "HYB-E1002",
    );

    assert_code(
        r#"
package main

fn f() error {
    return true
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn test_call_arg_scalar_param_with_error_rejected() {
    assert_code(
        r#"
package main

fn take_int(x int) {}

fn main() {
    var err error
    take_int(err)
}
"#,
        "HYB-E1002",
    );

    assert_code(
        r#"
package main

fn take_str(s string) {}

fn main() {
    var err error
    take_str(err)
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn test_call_arg_error_param_with_scalar_rejected() {
    assert_code(
        r#"
package main

fn take_err(e error) {}

fn main() {
    take_err(42)
}
"#,
        "HYB-E1002",
    );

    assert_code(
        r#"
package main

fn take_err(e error) {}

fn main() {
    take_err("some string")
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn test_error_variable_scalar_assign_rejected() {
    assert_code(
        r#"
package main

fn main() {
    var err error
    var n int = err
}
"#,
        "HYB-E1002",
    );

    assert_code(
        r#"
package main

fn main() {
    var n int
    var err error
    n = err
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn test_error_operators_rejected() {
    assert_code(
        r#"
package main

fn main() {
    var err error
    _ = err + 1
}
"#,
        "HYB-E1002",
    );

    assert_code(
        r#"
package main

fn main() {
    var err error
    _ = err == 42
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn test_error_method_undefined_rejected() {
    assert_code(
        r#"
package main

fn main() {
    var err error
    err.unknown_method()
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn test_positive_controls_valid_error_operations() {
    assert_ok(
        r#"
package main

fn f() error {
    var err error
    return err
}

fn take_error(e error) {}
fn take_any(a any) {}

fn main() {
    var err error
    take_error(err)
    take_any(err)
    _ = err == nil
    _ = err != nil
    _ = err.error()
}
"#);
}
