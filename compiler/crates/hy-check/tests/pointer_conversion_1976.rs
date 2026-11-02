//! pointer_conversion_1976.rs — regression for issue #1976.
//!
//! `conversion_kinds_conflict` did not model `Pointer`, so `int(p)` (and
//! `float64(p)`, `string(p)`) on a `*int` passed `hyb check` and failed
//! `go build` with "cannot convert p (variable of type *int) to type int".
//! Go only converts between pointers; an interface target is handled
//! separately and stays accepted.

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
fn rejects_int_conversion_from_pointer() {
    assert_reject(
        r#"fn main() {
    x := 42
    p := &x
    val := int(p)
    fmt.println(val)
}
"#,
    );
}

#[test]
fn rejects_float_conversion_from_pointer() {
    assert_reject(
        r#"fn main() {
    x := 42
    p := &x
    val := float64(p)
    fmt.println(val)
}
"#,
    );
}

#[test]
fn rejects_string_conversion_from_pointer() {
    assert_reject(
        r#"fn main() {
    x := 42
    p := &x
    val := string(p)
    fmt.println(val)
}
"#,
    );
}

#[test]
fn rejects_slice_conversion_from_pointer() {
    assert_reject(
        r#"fn main() {
    x := 42
    p := &x
    val := []byte(p)
    fmt.println(len(val))
}
"#,
    );
}

#[test]
fn accepts_pointer_assignment_and_scalar_conversions() {
    assert_accept(
        r#"fn main() {
    x := 42
    p := &x
    q := p
    f := float64(x)
    n := int(f)
    fmt.println(q != nil)
    fmt.println(n)
}
"#,
    );
}
