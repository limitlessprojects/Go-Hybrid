//! errors_wrap_operand_kinds_3572.rs — `errors.wrap(err, msg)` lowers to
//! `fmt.Errorf("%s: %w", msg, err)`, which Go builds for ANY operand types
//! and then prints `%!w(int=5)` / `%!s(int=7)` at run time. The checker only
//! counted arguments (#657); it now rejects operands whose resolved kind can
//! never be an `error` (arg 1) or a `string` (arg 2). Issue #3572.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn e1002_count(r: &hy_check::CheckResult) -> usize {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error && d.code == "HYB-E1002")
        .count()
}

fn wrap_src(call: &str) -> String {
    format!(
        "fn main() {{\n    base := errors.new(\"disk full\")\n    _ = base\n    e := {call}\n    fmt.println(e)\n}}\n"
    )
}

#[test]
fn non_error_first_operand_is_rejected() {
    for call in [
        "errors.wrap(5, \"ctx\")",
        "errors.wrap(\"not an error\", \"ctx\")",
        "errors.wrap(true, \"ctx\")",
        "errors.wrap(1.5, \"ctx\")",
        "errors.wrap([]int{1}, \"ctx\")",
    ] {
        let r = check(&wrap_src(call));
        assert_eq!(r.verdict, Verdict::Reject, "{call}: {:?}", r.diagnostics);
        assert_eq!(e1002_count(&r), 1, "{call}: {:?}", r.diagnostics);
    }
}

#[test]
fn non_string_message_is_rejected() {
    for call in [
        "errors.wrap(base, 7)",
        "errors.wrap(base, true)",
        "errors.wrap(base, 2.5)",
        "errors.wrap(base, []string{\"a\"})",
    ] {
        let r = check(&wrap_src(call));
        assert_eq!(r.verdict, Verdict::Reject, "{call}: {:?}", r.diagnostics);
        assert_eq!(e1002_count(&r), 1, "{call}: {:?}", r.diagnostics);
    }
}

#[test]
fn typed_locals_are_rejected_too() {
    let r = check(
        "fn main() {\n    var n int = 3\n    var s string = \"x\"\n    base := errors.new(\"b\")\n    a := errors.wrap(n, \"ctx\")\n    b := errors.wrap(s, \"ctx\")\n    c := errors.wrap(base, n)\n    fmt.println(a, b, c)\n}\n",
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert_eq!(e1002_count(&r), 3, "{:?}", r.diagnostics);
}

#[test]
fn valid_wraps_still_accepted() {
    for call in [
        "errors.wrap(base, \"ctx\")",
        "errors.wrap(errors.wrap(base, \"inner\"), \"outer\")",
        "errors.wrap(base, \"a\" + \"b\")",
    ] {
        let r = check(&wrap_src(call));
        assert_eq!(r.verdict, Verdict::Accept, "{call}: {:?}", r.diagnostics);
    }
    let r = check(
        "fn load() error {\n    err := fmt.errorf(\"disk full\")\n    return errors.wrap(err, \"reading config\")\n}\n\nfn main() {\n    fmt.println(load())\n}\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}
