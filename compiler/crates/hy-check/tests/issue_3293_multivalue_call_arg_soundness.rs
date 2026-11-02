//! Regression tests for #3293:
//! Multi-value function call arguments f(pair()) must enforce scalar width,
//! fixed-size array length, and float-int type soundness (Row 13 BUILDABLE).

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let f = parse_file("test.hy", src).expect("parse");
    check_file(&f).diagnostics.into_iter().map(|d| d.code).collect()
}

#[test]
fn test_multivalue_fn_arg_scalar_mismatch() {
    let src = r#"
package main

fn pair() (int64, string) {
    return 1, "ok"
}

fn consumer(a int32, b string) {
    _ = a
    _ = b
}

fn main() {
    consumer(pair())
}
"#;
    let c = codes(src);
    assert!(
        c.iter().any(|code| code == "HYB-E1002"),
        "expected HYB-E1002 for scalar mismatch, got {:?}",
        c
    );
}

#[test]
fn test_multivalue_fn_arg_float_scalar_mismatch() {
    let src = r#"
package main

fn pair() (float32, int) {
    var x float32 = 1.0
    return x, 2
}

fn consumer(a float64, b int) {
    _ = a
    _ = b
}

fn main() {
    consumer(pair())
}
"#;
    let c = codes(src);
    assert!(
        c.iter().any(|code| code == "HYB-E1002"),
        "expected HYB-E1002 for float scalar mismatch, got {:?}",
        c
    );
}

#[test]
fn test_multivalue_fn_arg_array_length_mismatch() {
    let src = r#"
package main

fn pair() ([3]int, string) {
    return [3]int{1, 2, 3}, "ok"
}

fn consumer(a [2]int, b string) {
    _ = a
    _ = b
}

fn main() {
    consumer(pair())
}
"#;
    let c = codes(src);
    assert!(
        c.iter().any(|code| code == "HYB-E1002"),
        "expected HYB-E1002 for array length mismatch, got {:?}",
        c
    );
}

#[test]
fn test_multivalue_fn_arg_int_to_float_conflict() {
    let src = r#"
package main

fn pair(n int) (int, string) {
    return n, "ok"
}

fn consumer(a float64, b string) {
    _ = a
    _ = b
}

fn main() {
    consumer(pair(10))
}
"#;
    let c = codes(src);
    assert!(
        c.iter().any(|code| code == "HYB-E1002"),
        "expected HYB-E1002 for int-to-float conflict, got {:?}",
        c
    );
}

#[test]
fn test_multivalue_call_matching_accepted() {
    let src = r#"
package main

fn pair() (int32, string) {
    var x int32 = 1
    return x, "ok"
}

fn consumer(a int32, b string) {
    _ = a
    _ = b
}

fn main() {
    consumer(pair())
}
"#;
    let c = codes(src);
    assert!(
        c.is_empty(),
        "expected 0 diagnostics for matching multi-value call args, got {:?}",
        c
    );
}

#[test]
fn test_multivalue_inline_fn_lit_call_scalar_mismatch() {
    let src = r#"
package main

fn consumer(a int32, b string) {
    _ = a
    _ = b
}

fn main() {
    consumer((fn() (int64, string) {
        return 1, "ok"
    })())
}
"#;
    let c = codes(src);
    assert!(
        c.iter().any(|code| code == "HYB-E1002"),
        "expected HYB-E1002 for inline fn lit scalar mismatch, got {:?}",
        c
    );
}
