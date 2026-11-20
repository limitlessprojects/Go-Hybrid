//! issue_4600_associated_fn_buildable_soundness.rs — regression for issue #4600.
//!
//! Associated function calls (`Type.fn(args...)`) lower to package-level Go
//! identifiers (`Type_name(args...)`). In `buildable.rs`, `Expr::Field` callees
//! must validate call arity, parameter types, variadic spread restrictions,
//! and fallible `?` single-value unwraps against `self.ck.fns`, matching the
//! compile-time guarantees enforced on top-level functions (Row 13 BUILDABLE).

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

const COUNTER_DEF: &str = r#"struct counter {
    n int
}

fn counter.make(init int) counter {
    return counter{n: init}
}

fn counter.pair() (int, int) {
    return 1, 2
}
"#;

const WORKER_DEF: &str = r#"struct worker {}

fn worker.run() error {
    return nil
}

fn worker.one() (int, error) {
    return 1, nil
}

fn worker.multi() (int, int, error) {
    return 1, 2, nil
}
"#;

#[test]
fn rejects_associated_fn_too_few_arguments() {
    assert_reject(&format!(
        "{COUNTER_DEF}
fn main() {{
    c := counter.make()
    _ = c
}}
"
    ));
}

#[test]
fn rejects_associated_fn_too_many_arguments() {
    assert_reject(&format!(
        "{COUNTER_DEF}
fn main() {{
    c := counter.make(1, 2)
    _ = c
}}
"
    ));
}

#[test]
fn rejects_associated_fn_argument_type_mismatch() {
    assert_reject(&format!(
        "{COUNTER_DEF}
fn main() {{
    c := counter.make(\"wrong\")
    _ = c
}}
"
    ));
}

#[test]
fn rejects_associated_fn_variadic_spread() {
    assert_reject(&format!(
        "{COUNTER_DEF}
fn main() {{
    xs := []int{{1}}
    c := counter.make(xs...)
    _ = c
}}
"
    ));
}

#[test]
fn rejects_associated_fn_void_try_in_binding() {
    assert_reject(&format!(
        "{WORKER_DEF}
fn f() error {{
    x := worker.run()?
    _ = x
    return nil
}}
"
    ));
}

#[test]
fn rejects_associated_fn_void_try_as_call_argument() {
    assert_reject(&format!(
        "{WORKER_DEF}
fn take(n int) {{ print(n) }}

fn f() error {{
    take(worker.run()?)
    return nil
}}
"
    ));
}

#[test]
fn rejects_associated_fn_multi_value_try_in_binary_op() {
    assert_reject(&format!(
        "{WORKER_DEF}
fn f() error {{
    x := 1 + worker.multi()?
    _ = x
    return nil
}}
"
    ));
}

#[test]
fn rejects_associated_fn_multi_value_try_as_index() {
    assert_reject(&format!(
        "{WORKER_DEF}
fn f() error {{
    s := []int{{1, 2, 3}}
    x := s[worker.multi()?]
    _ = x
    return nil
}}
"
    ));
}

#[test]
fn accepts_valid_associated_fn_call() {
    assert_accept(&format!(
        "{COUNTER_DEF}
fn main() {{
    c := counter.make(42)
    _ = c
}}
"
    ));
}

#[test]
fn accepts_associated_fn_single_value_try_binding() {
    assert_accept(&format!(
        "{WORKER_DEF}
fn f() error {{
    x := worker.one()?
    _ = x
    return nil
}}
"
    ));
}

#[test]
fn accepts_associated_fn_multi_value_try_binding() {
    assert_accept(&format!(
        "{WORKER_DEF}
fn f() error {{
    a, b := worker.multi()?
    _ = a
    _ = b
    return nil
}}
"
    ));
}

#[test]
fn accepts_associated_fn_void_try_as_statement() {
    assert_accept(&format!(
        "{WORKER_DEF}
fn f() error {{
    worker.run()?
    return nil
}}
"
    ));
}

const GENERIC_BOX: &str = r#"struct box[t any] {
    v t
}

fn box.new[t any](val t) box[t] {
    return box[t]{v: val}
}
"#;

#[test]
fn accepts_generic_associated_fn_explicit_instantiation() {
    assert_accept(&format!(
        "{GENERIC_BOX}
fn main() {{
    b := box.new[int](42)
    _ = b
}}
"
    ));
}

#[test]
fn rejects_generic_associated_fn_wrong_arg_count() {
    assert_reject(&format!(
        "{GENERIC_BOX}
fn main() {{
    b := box.new[int](42, 99)
    _ = b
}}
"
    ));
}
