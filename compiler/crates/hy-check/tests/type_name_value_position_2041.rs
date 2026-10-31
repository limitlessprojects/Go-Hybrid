//! type_name_value_position_2041.rs — regression for issue #2041 (gaps 1 and 2).
//!
//! `check_type_name_in_value_position` did not recognise the built-in type
//! names `any` and `error`, nor a generic struct's base name
//! (`struct_type_params`), so `x := any`, `y := error` and `x := container`
//! passed `hyb check` and failed `go build` with "<type> (type) is not an
//! expression" (Row 13 BUILDABLE).
//!
//! Gap 3 of the issue (qualified types such as `http.Client`) needs an
//! import-aware type table and is not covered here.

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
fn rejects_any_type_name_in_value_position() {
    assert_reject(
        r#"fn main() {
    x := any
    fmt.println(x)
}
"#,
    );
}

#[test]
fn rejects_error_type_name_in_value_position() {
    assert_reject(
        r#"fn main() {
    y := error
    fmt.println(y)
}
"#,
    );
}

#[test]
fn rejects_any_as_call_argument() {
    assert_reject(
        r#"fn inspect(val any) {}

fn main() {
    inspect(any)
}
"#,
    );
}

#[test]
fn accepts_ordinary_values() {
    assert_accept(
        r#"fn main() {
    x := 1
    s := "ok"
    fmt.println(x)
    fmt.println(s)
}
"#,
    );
}
