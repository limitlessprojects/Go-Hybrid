//! Test suite for Issue #3057: set[T] kind checking in returns, call arguments,
//! and function signatures.
//!
//! Row 13 (BUILDABLE) / SPEC §2.3 T9.

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
fn test_set_return_scalar_rejected() {
    assert_code(
        r#"
package main

fn f() int {
    var s set[int]
    return s
}
"#,
        "HYB-E1002",
    );

    assert_code(
        r#"
package main

fn f() string {
    var s set[int]
    return s
}
"#,
        "HYB-E1002",
    );

    assert_code(
        r#"
package main

fn f() bool {
    var s set[int]
    return s
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn test_scalar_return_in_set_fn_rejected() {
    assert_code(
        r#"
package main

fn f() set[int] {
    return 42
}
"#,
        "HYB-E1002",
    );

    assert_code(
        r#"
package main

fn f() set[int] {
    return "hello"
}
"#,
        "HYB-E1002",
    );

    assert_code(
        r#"
package main

fn f() set[int] {
    return true
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn test_mismatched_set_return_rejected() {
    assert_code(
        r#"
package main

fn f() set[int] {
    var s set[string]
    return s
}
"#,
        "HYB-E1002",
    );

    assert_code(
        r#"
package main

fn f() set[string] {
    var s set[int]
    return s
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn test_call_arg_set_param_with_scalar_rejected() {
    assert_code(
        r#"
package main

fn take_set(s set[int]) {}

fn main() {
    take_set(42)
}
"#,
        "HYB-E1002",
    );

    assert_code(
        r#"
package main

fn take_set(s set[int]) {}

fn main() {
    take_set("hello")
}
"#,
        "HYB-E1002",
    );

    assert_code(
        r#"
package main

fn take_set(s set[int]) {}

fn main() {
    take_set(true)
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn test_call_arg_scalar_param_with_set_rejected() {
    assert_code(
        r#"
package main

fn take_int(x int) {}

fn main() {
    var s set[int]
    take_int(s)
}
"#,
        "HYB-E1002",
    );

    assert_code(
        r#"
package main

fn take_str(s string) {}

fn main() {
    var st set[int]
    take_str(st)
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn test_call_arg_mismatched_set_param_rejected() {
    assert_code(
        r#"
package main

fn take_set_int(s set[int]) {}

fn main() {
    var s set[string]
    take_set_int(s)
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn test_invalid_set_method_rejected() {
    assert_code(
        r#"
package main

fn main() {
    var s set[int]
    s.unknown_method()
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn test_set_method_arity_rejected() {
    assert_code(
        r#"
package main

fn main() {
    var s set[int]
    s.insert()
}
"#,
        "HYB-E1002",
    );

    assert_code(
        r#"
package main

fn main() {
    var s set[int]
    _ = s.len(42)
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn test_set_compared_with_nil_rejected() {
    assert_code(
        r#"
package main

fn main() {
    var s set[int]
    _ = s == nil
}
"#,
        "HYB-E1002",
    );

    assert_code(
        r#"
package main

fn main() {
    var s set[int]
    _ = nil == s
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn test_set_arithmetic_rejected() {
    assert_code(
        r#"
package main

fn main() {
    var s set[int]
    _ = s + 1
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn test_set_delete_mismatched_key_rejected() {
    assert_code(
        r#"
package main

fn main() {
    var s set[int]
    delete(s, "bad")
}
"#,
        "HYB-E2005",
    );
}

#[test]
fn test_positive_controls_valid_set_operations() {
    assert_ok(
        r#"
package main

fn f() set[int] {
    var s set[int]
    return s
}

fn g() set[int] {
    return set[int]{}
}

fn take_set(s set[int]) {}

fn main() {
    var s set[int]
    take_set(s)
    take_set(set[int]{})
    _ = s.contains(10)
    _ = s.len()
    s.remove(10)
    delete(s, 10)
    for v in s {
        _ = v + 1
    }
}
"#,
    );
}
