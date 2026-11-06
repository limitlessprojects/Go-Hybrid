//! errors_wrap_shadowed_fmt.rs: regression tests for Row 13 (BUILDABLE) gap in errors.wrap.
//!
//! Under GUARANTEES.md Row 13 (BUILDABLE), programs accepted by `hy-check` must emit
//! buildable Go code. In `hy-emit`, `errors.wrap(err, msg)` lowers to `fmt.Errorf("%s: %w", msg, err)`
//! with an implicit `import "fmt"`.
//! When an in-scope parameter, local binding, generic type parameter, or package-level declaration
//! named `fmt` exists, emitted Go resolves `fmt.Errorf` to the shadowing binding or fails with
//! `fmt redeclared in this block`, causing downstream `go build` to fail with `fmt.Errorf undefined`.
//!
//! `hy-check` must reject calls to `errors.wrap` when `fmt` is shadowed with `HYB-E1002`.

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

fn rejects(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {:?}, got {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(cs.is_empty(), "must accept {:?}, got {:?}", src, cs);
}

#[test]
fn errors_wrap_param_named_fmt_rejects() {
    rejects("import \"errors\"\nfn annotate(fmt string, err error) error {\n    return errors.wrap(err, fmt)\n}\nfn main() {}\n");
}

#[test]
fn errors_wrap_local_named_fmt_rejects() {
    rejects("import \"errors\"\nfn annotate(err error) error {\n    fmt := \"ctx\"\n    return errors.wrap(err, fmt)\n}\nfn main() {}\n");
}

#[test]
fn errors_wrap_closure_param_named_fmt_rejects() {
    rejects("import \"errors\"\nfn annotate(err error) error {\n    cb := fn(fmt string) error {\n        return errors.wrap(err, fmt)\n    }\n    return cb(\"ctx\")\n}\nfn main() {}\n");
}

#[test]
fn errors_wrap_type_param_named_fmt_rejects() {
    rejects("import \"errors\"\nfn annotate[fmt any](err error, msg string) error {\n    return errors.wrap(err, msg)\n}\nfn main() {}\n");
}

#[test]
fn errors_wrap_package_const_named_fmt_rejects() {
    rejects("import \"errors\"\nconst fmt = 1\nfn annotate(err error) error {\n    return errors.wrap(err, \"ctx\")\n}\nfn main() {}\n");
}

#[test]
fn errors_wrap_package_struct_named_fmt_rejects() {
    rejects("import \"errors\"\nstruct fmt { x int }\nfn annotate(err error) error {\n    return errors.wrap(err, \"ctx\")\n}\nfn main() {}\n");
}

#[test]
fn errors_wrap_legal_unshadowed_accepts() {
    accepts("import \"errors\"\nfn annotate(ctx string, err error) error {\n    return errors.wrap(err, ctx)\n}\nfn main() {}\n");
}

#[test]
fn errors_wrap_legal_sibling_param_accepts() {
    accepts("import \"errors\"\nfn annotate(format string, err error) error {\n    return errors.wrap(err, format)\n}\nfn main() {}\n");
}
