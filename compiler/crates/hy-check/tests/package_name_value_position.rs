//! A bare package root used as a value passes `hyb check` today and the
//! emitted Go fails `go build` with `use of package X not in selector`.
//! Row 13 (BUILDABLE).

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {diags:?}"));
    check_file(&ast)
}

fn codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_rejected(src: &str) {
    let r = check(src);
    let found = codes(&r);
    assert_eq!(r.verdict, Verdict::Reject, "expected reject, got {found:?}");
    assert!(
        found.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {found:?}"
    );
}

fn assert_accepted(src: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept, got {:?}",
        codes(&r)
    );
}

#[test]
fn bare_builtin_package_rejected() {
    assert_rejected("fn main() {\n    fmt.println(fmt)\n}\n");
}

#[test]
fn imported_package_bound_to_variable_rejected() {
    assert_rejected(
        "import \"strings\"\n\
         fn main() {\n\
         \x20   fmt.println(strings.to_upper(\"x\"))\n\
         \x20   y := strings\n\
         \x20   fmt.println(y)\n\
         }\n",
    );
}

#[test]
fn package_as_call_argument_rejected() {
    assert_rejected(
        "fn take(x any) { _ = x }\n\
         fn main() {\n\
         \x20   take(fmt)\n\
         }\n",
    );
}

#[test]
fn package_selector_stays_accepted() {
    assert_accepted("fn main() {\n    fmt.println(fmt.sprint(\"x\"))\n}\n");
}

#[test]
fn local_shadow_stays_accepted() {
    assert_accepted(
        "import \"strings\"\n\
         fn main() {\n\
         \x20   fmt.println(strings.to_upper(\"x\"))\n\
         \x20   {\n\
         \x20       strings := 1\n\
         \x20       fmt.println(strings)\n\
         \x20   }\n\
         }\n",
    );
}

#[test]
fn package_scope_fn_shadow_stays_accepted() {
    assert_accepted("fn path() int { return 1 }\nfn main() {\n    fmt.println(path())\n}\n");
}

#[test]
fn parameter_shadow_stays_accepted() {
    assert_accepted(
        "import \"strings\"\n\
         fn f(strings int) int { return strings }\n\
         fn main() {\n\
         \x20   fmt.println(strings.to_upper(\"x\"))\n\
         \x20   fmt.println(f(1))\n\
         }\n",
    );
}
