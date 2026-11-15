//! Regression tests for #3572:
//! `errors.wrap(err, message)` operand kind validation.
//!
//! SPEC-LANGUAGE E2: `errors.wrap(err, "context")`.
//!
//! Lowered to `fmt.Errorf("%s: %w", message, err)`. `hy-check` must reject
//! operands whose resolved kind can never be an `error` (argument 1) or a
//! `string` (argument 2) with HYB-E1002, preventing corrupt runtime error chains.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("errors_wrap_test.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn count_e1002(r: &hy_check::CheckResult) -> usize {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error && d.code == "HYB-E1002")
        .count()
}

fn wrap_program(stmt: &str) -> String {
    format!(
        "fn main() {{\n    base := errors.new(\"disk full\")\n    _ = base\n    e := {stmt}\n    fmt.println(e)\n}}\n"
    )
}

#[test]
fn non_error_first_argument_is_rejected() {
    let cases = [
        "errors.wrap(5, \"ctx\")",
        "errors.wrap(\"not an error\", \"ctx\")",
        "errors.wrap(true, \"ctx\")",
        "errors.wrap(1.5, \"ctx\")",
        "errors.wrap([]int{1}, \"ctx\")",
    ];
    for call in cases {
        let r = check(&wrap_program(call));
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected reject for non-error arg: {call}"
        );
        assert_eq!(
            count_e1002(&r),
            1,
            "expected HYB-E1002 for non-error arg: {call}, got {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn non_string_second_argument_is_rejected() {
    let cases = [
        "errors.wrap(base, 7)",
        "errors.wrap(base, true)",
        "errors.wrap(base, 2.5)",
        "errors.wrap(base, []string{\"a\"})",
    ];
    for call in cases {
        let r = check(&wrap_program(call));
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected reject for non-string message: {call}"
        );
        assert_eq!(
            count_e1002(&r),
            1,
            "expected HYB-E1002 for non-string message: {call}, got {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn typed_local_variables_are_rejected() {
    let src = r#"
fn main() {
    var n int = 3
    var s string = "x"
    base := errors.new("b")
    a := errors.wrap(n, "ctx")
    b := errors.wrap(s, "ctx")
    c := errors.wrap(base, n)
    fmt.println(a, b, c)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert_eq!(
        count_e1002(&r),
        3,
        "expected 3 HYB-E1002 diagnostics for typed locals, got {:?}",
        r.diagnostics
    );
}

#[test]
fn valid_errors_wrap_calls_stay_accepted() {
    let valid_calls = [
        "errors.wrap(base, \"ctx\")",
        "errors.wrap(errors.wrap(base, \"inner\"), \"outer\")",
        "errors.wrap(base, \"a\" + \"b\")",
    ];
    for call in valid_calls {
        let r = check(&wrap_program(call));
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "expected accept for valid call: {call}, got {:?}",
            r.diagnostics
        );
    }
}
