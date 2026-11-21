//! issue_4591_method_try_single_value_soundness.rs — regression for issue #4591.
//!
//! `non_single_value_count` in `buildable.rs` unwraps `Expr::Try` to detect void
//! and multi-value calls used in single-value positions, but previously only
//! inspected top-level functions (`Expr::Ident`) and closure literals (`Expr::FnLit`).
//! Declared struct methods (`Expr::Field`) were omitted and returned `None`, allowing
//! void method try expressions (`x := w.run()?`) and multi-value method try
//! expressions (`1 + w.multi()?`) to pass `hyb check` and fail `go build` with
//! `(no value) used as value` or `multiple-value in single-value context` (Row 13 BUILDABLE).

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
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

const STRUCT_DEF: &str = r#"struct worker {}

fn (w worker) run() error {
    return nil
}

fn (w worker) multi() (int, int, error) {
    return 1, 2, nil
}

fn (w worker) one() (int, error) {
    return 1, nil
}
"#;

#[test]
fn rejects_void_method_try_as_binding() {
    assert_reject(&format!(
        "{STRUCT_DEF}
fn f() error {{
    var w worker
    x := w.run()?
    _ = x
    return nil
}}
"
    ));
}

#[test]
fn rejects_void_method_try_as_call_argument() {
    assert_reject(&format!(
        "{STRUCT_DEF}
fn take(v int) {{ print(v) }}

fn f() error {{
    var w worker
    take(w.run()?)
    return nil
}}
"
    ));
}

#[test]
fn rejects_void_method_try_as_slice_bound() {
    assert_reject(&format!(
        "{STRUCT_DEF}
fn f() error {{
    var w worker
    s := []int{{1, 2, 3}}
    sub := s[w.run()?:2]
    _ = sub
    return nil
}}
"
    ));
}

#[test]
fn rejects_void_method_try_as_array_index() {
    assert_reject(&format!(
        "{STRUCT_DEF}
fn f() error {{
    var w worker
    s := []int{{1, 2, 3}}
    x := s[w.run()?]
    _ = x
    return nil
}}
"
    ));
}

#[test]
fn rejects_void_method_try_in_binary_op() {
    assert_reject(&format!(
        "{STRUCT_DEF}
fn f() error {{
    var w worker
    x := 1 + w.run()?
    _ = x
    return nil
}}
"
    ));
}

#[test]
fn rejects_multi_value_method_try_as_index() {
    assert_reject(&format!(
        "{STRUCT_DEF}
fn f() error {{
    var w worker
    s := []int{{1, 2, 3}}
    x := s[w.multi()?]
    _ = x
    return nil
}}
"
    ));
}

#[test]
fn rejects_multi_value_method_try_in_binary_op() {
    assert_reject(&format!(
        "{STRUCT_DEF}
fn f() error {{
    var w worker
    x := 1 + w.multi()?
    _ = x
    return nil
}}
"
    ));
}

#[test]
fn rejects_multi_value_method_try_as_call_argument() {
    assert_reject(&format!(
        "{STRUCT_DEF}
fn take(v int) {{ print(v) }}

fn f() error {{
    var w worker
    take(w.multi()?)
    return nil
}}
"
    ));
}

#[test]
fn accepts_single_value_method_try_binding() {
    assert_accept(&format!(
        "{STRUCT_DEF}
fn f() error {{
    var w worker
    x := w.one()?
    _ = x
    return nil
}}
"
    ));
}

#[test]
fn accepts_single_value_method_try_as_call_argument() {
    assert_accept(&format!(
        "{STRUCT_DEF}
fn take(v int) {{ print(v) }}

fn f() error {{
    var w worker
    take(w.one()?)
    return nil
}}
"
    ));
}

#[test]
fn accepts_multi_value_method_try_binding() {
    assert_accept(&format!(
        "{STRUCT_DEF}
fn f() error {{
    var w worker
    a, b := w.multi()?
    _ = a
    _ = b
    return nil
}}
"
    ));
}

#[test]
fn accepts_void_method_try_as_statement() {
    assert_accept(&format!(
        "{STRUCT_DEF}
fn f() error {{
    var w worker
    w.run()?
    return nil
}}
"
    ));
}

#[test]
fn rejects_pointer_receiver_void_method_try() {
    assert_reject(&format!(
        "{STRUCT_DEF}
fn f() error {{
    var w worker
    p := &w
    x := p.run()?
    _ = x
    return nil
}}
"
    ));
}

#[test]
fn accepts_pointer_receiver_multi_value_try_binding() {
    assert_accept(&format!(
        "{STRUCT_DEF}
fn f() error {{
    var w worker
    p := &w
    a, b := p.multi()?
    _ = a
    _ = b
    return nil
}}
"
    ));
}
