//! slice_pop_1979.rs — regression for issue #1979.
//!
//! `check_call_operands` routed every `x.pop()` call to
//! `check_slice_pop_operands` before the missing-member check, and that helper
//! ignored `Struct`/`Pointer`. A struct or pointer without a declared `pop`
//! method therefore passed `hyb check` and failed `go build`.

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_reject(src: &str) {
    let found = codes(src);
    assert!(
        found.iter().any(|code| code == "HYB-E1002"),
        "expected HYB-E1002, got {found:?} for:\n{src}"
    );
}

fn assert_accept(src: &str) {
    let found = codes(src);
    assert!(
        found.is_empty(),
        "expected acceptance, got {found:?} for:\n{src}"
    );
}

#[test]
fn rejects_pop_on_struct_without_method() {
    assert_reject(
        r#"struct counter {
    val int
}

fn main() {
    c := counter{val: 10}
    c.pop()
    fmt.println(c.val)
}
"#,
    );
}

#[test]
fn rejects_pop_on_pointer_without_method() {
    assert_reject(
        r#"struct counter {
    val int
}

fn f(c *counter) {
    c.pop()
}

fn main() {
    c := counter{val: 10}
    f(&c)
}
"#,
    );
}

#[test]
fn accepts_declared_pop_method_on_struct() {
    assert_accept(
        r#"struct counter {
    val int
}

fn (c *counter) pop() int {
    return c.val
}

fn main() {
    c := counter{val: 10}
    x := c.pop()
    fmt.println(x)
}
"#,
    );
}

#[test]
fn accepts_builtin_pop_on_slice() {
    assert_accept(
        r#"fn main() {
    s := []int{1, 2, 3}
    x := s.pop()
    fmt.println(x)
}
"#,
    );
}
