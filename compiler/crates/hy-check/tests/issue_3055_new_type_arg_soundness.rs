//! Regression test for Issue #3055:
//! Non-literal value expressions passed to builtin `new(...)` must be rejected (HYB-E1002).
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
fn test_new_rejects_local_variable() {
    assert_e1002(
        r#"
fn main() {
    x := 1
    println(x)
    _ = new(x)
}
"#,
    );
}

#[test]
fn test_new_rejects_fn_parameter() {
    assert_e1002(
        r#"
fn test(n int) {
    println(n)
    _ = new(n)
}
fn main() {
    test(1)
}
"#,
    );
}

#[test]
fn test_new_rejects_call_expression() {
    assert_e1002(
        r#"
fn f() int {
    return 1
}
fn main() {
    _ = new(f())
}
"#,
    );
}

#[test]
fn test_new_rejects_binary_expression() {
    assert_e1002(
        r#"
fn main() {
    _ = new(1 + 2)
}
"#,
    );
}

#[test]
fn test_new_rejects_field_on_local_variable() {
    assert_e1002(
        r#"
struct point {
    x int
}
fn main() {
    pt := point{x: 1}
    println(pt.x)
    _ = new(pt.x)
}
"#,
    );
}

#[test]
fn test_new_rejects_index_expression() {
    assert_e1002(
        r#"
fn main() {
    xs := []int{1}
    println(xs[0])
    _ = new(xs[0])
}
"#,
    );
}

#[test]
fn test_new_rejects_top_level_function() {
    assert_e1002(
        r#"
fn helper() {}
fn main() {
    _ = new(helper)
}
"#,
    );
}

#[test]
fn test_new_rejects_top_level_constant() {
    assert_e1002(
        r#"
const limit = 100
fn main() {
    _ = new(limit)
}
"#,
    );
}

#[test]
fn test_new_rejects_boolean_ident() {
    assert_e1002(
        r#"
fn main() {
    _ = new(true)
}
"#,
    );
}

#[test]
fn test_new_accepts_valid_types() {
    assert_ok("fn main() { _ = new(int) }");
    assert_ok("fn main() { _ = new(string) }");
    assert_ok("fn main() { _ = new(bool) }");
    assert_ok("fn main() { _ = new(float64) }");
    assert_ok("struct point { x int }\nfn main() { _ = new(point) }");
    assert_ok("enum status { ok, fail }\nfn main() { _ = new(status) }");
    assert_ok("type count = int\nfn main() { _ = new(count) }");
    assert_ok("fn main() { _ = new([]int) }");
    assert_ok("fn main() { _ = new(map[string]int{}) }");
    assert_ok("fn create[t any]() { _ = new(t) }\nfn main() { create[int]() }");
}
