//! Regression tests for `...` spread arguments on curated `fmt` variadic
//! members (print/scan/append families). Go accepts a spread only as the
//! final argument, only after exactly the fixed leading parameters, and only
//! when the operand is assignable to `[]any`. Every other shape used to pass
//! `hyb check` and fail `go build` (HYB-E1002, Row 13 BUILDABLE).

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_rejects_e1002(src: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected reject for: {src}, got: {:?}", r.diagnostics);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for: {src}, got: {:?}",
        r.diagnostics
    );
}

fn assert_accepts(src: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept for: {src}, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn fmt_spread_of_non_any_slice_rejected_e1002() {
    let cases = [
        "fn main() {\n    xs := []string{\"a\"}\n    fmt.println(xs...)\n}",
        "fn main() {\n    xs := []int{1, 2}\n    fmt.printf(\"%d %d\\n\", xs...)\n}",
        "fn main() {\n    xs := []error{}\n    fmt.println(xs...)\n}",
        "fn main() {\n    xs := []int{1}\n    s := fmt.sprint(xs...)\n    fmt.println(s)\n}",
    ];
    for src in cases {
        assert_rejects_e1002(src);
    }
}

#[test]
fn fmt_spread_of_non_slice_rejected_e1002() {
    assert_rejects_e1002("fn main() {\n    n := 3\n    fmt.println(n...)\n}");
}

#[test]
fn fmt_spread_not_final_argument_rejected_e1002() {
    let cases = [
        "fn main() {\n    xs := []any{1}\n    e := fmt.errorf(\"x %d\", xs..., 2)\n    fmt.println(e)\n}",
        "fn main() {\n    xs := []any{1}\n    s := fmt.sprint(xs..., xs...)\n    fmt.println(s)\n}",
    ];
    for src in cases {
        assert_rejects_e1002(src);
    }
}

#[test]
fn fmt_spread_with_wrong_leading_argument_count_rejected_e1002() {
    let cases = [
        // individual values mixed with a spread slice
        "fn main() {\n    xs := []any{1}\n    fmt.println(\"a\", xs...)\n}",
        "fn main() {\n    xs := []any{1}\n    fmt.printf(\"%v %v\", 1, xs...)\n}",
        // fixed leading parameters missing
        "fn main() {\n    xs := []any{1}\n    fmt.fprintln(xs...)\n}",
        "fn main() {\n    xs := []any{\"%d\", 1}\n    fmt.printf(xs...)\n}",
        "fn main() {\n    xs := []any{1}\n    fmt.fprintf(os.stdout, xs...)\n}",
        "fn main() {\n    xs := []any{1}\n    n, err := fmt.sscan(xs...)\n    fmt.println(n, err)\n}",
    ];
    for src in cases {
        assert_rejects_e1002(src);
    }
}

#[test]
fn fmt_spread_of_any_slice_accepted() {
    assert_accepts(
        r#"type list = []any

fn vals() []any {
    return []any{1, "b"}
}

fn main() {
    xs := []any{1, "two"}
    fmt.println(xs...)
    fmt.print(xs[1:]...)
    fmt.printf("%v %v\n", xs...)
    s := fmt.sprintf("%v-%v", xs...)
    fmt.println(s, fmt.sprint(vals()...), fmt.sprintln(xs...))
    e := fmt.errorf("e %v %v", xs...)
    fmt.println(e)
    fmt.fprintln(os.stdout, xs...)
    fmt.fprintf(os.stdout, "%v %v\n", xs...)
    fmt.eprintln(xs...)
    var l list = []any{1, 2}
    fmt.println(l...)
}
"#,
    );
}

#[test]
fn fmt_calls_without_spread_unaffected() {
    assert_accepts(
        r#"fn main() {
    xs := []string{"a", "b"}
    fmt.println(xs)
    fmt.println("a", 1, xs)
    fmt.printf("%v %d\n", xs, 2)
}
"#,
    );
}
