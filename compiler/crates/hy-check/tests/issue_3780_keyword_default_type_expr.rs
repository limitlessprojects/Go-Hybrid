//! Tests for Issue #3780: TypeExpr walking in go_keyword_names pass.
//! Under GUARANTEES.md Row 13 (BUILDABLE), any program accepted by `hyb check`
//! must emit Go code that compiles under `go build`.
//!
//! Declarations named `default` inside function types (`TypeExpr::Fn`) and
//! closure declarations nested in array length expressions (`TypeExpr::Array`)
//! must be rejected fail-closed with HYB-E1002.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).unwrap_or_else(|d| panic!("must parse: {d:?}"));
    check_file(&ast)
}

fn errors(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect()
}

fn assert_rejects(src: &str, what: &str) {
    let r = check(src);
    let errs = errors(&r);
    assert_eq!(r.verdict, Verdict::Reject, "must reject:\n{src}");
    let needle = format!("{what} named `default`");
    assert!(
        errs.iter()
            .any(|e| e.starts_with("HYB-E1002") && e.contains(&needle) && e.contains("Go keyword")),
        "expected HYB-E1002 `{needle}` for:\n{src}\ngot {errs:?}"
    );
}

#[test]
fn issue_3780_type_alias_fn_param_default_rejected() {
    let src = r#"
type cb = fn(default int) int

fn main() {
    var f cb = fn(x int) int { return x }
    fmt.println("{f(1)}")
}
"#;
    assert_rejects(src, "parameter");
}

#[test]
fn issue_3780_struct_field_fn_param_default_rejected() {
    let src = r#"
struct handler {
    callback fn(default int) int
}

fn main() {
    fmt.println("ok")
}
"#;
    assert_rejects(src, "parameter");
}

#[test]
fn issue_3780_fn_param_signature_default_rejected() {
    let src = r#"
fn run(f fn(default int) int) int {
    return f(1)
}

fn main() {
    fmt.println("{run(fn(x int) int { return x })}")
}
"#;
    assert_rejects(src, "parameter");
}

#[test]
fn issue_3780_fn_return_signature_default_rejected() {
    let src = r#"
fn make_cb() fn(default int) int {
    return fn(x int) int { return x }
}

fn main() {
    fmt.println("ok")
}
"#;
    assert_rejects(src, "parameter");
}

#[test]
fn issue_3780_local_var_fn_type_default_rejected() {
    let src = r#"
fn main() {
    var f fn(default int) int
    fmt.println("ok")
}
"#;
    assert_rejects(src, "parameter");
}

#[test]
fn issue_3780_closure_in_array_len_default_rejected() {
    let src = r#"
fn main() {
    var a [len([1]int{fn(default int) int { return default }(0)})]int
    fmt.println("{len(a)}")
}
"#;
    assert_rejects(src, "parameter");
}

#[test]
fn issue_3780_positive_controls_accepted() {
    let cases = [
        // Valid parameter name in fn type
        r#"
type cb = fn(fallback int) int

fn main() {
    var f cb = fn(x int) int { return x }
    fmt.println("{f(1)}")
}
"#,
        // Struct field with valid fn type
        r#"
struct handler {
    callback fn(dflt int) int
}

fn main() {
    h := handler{callback: fn(x int) int { return x }}
    fmt.println("{h.callback(42)}")
}
"#,
        // Select with default arm
        r#"
fn main() {
    ch := make(chan int, 1)
    select {
        v := <-ch: fmt.println("{v}")
        default: fmt.println("empty")
    }
}
"#,
        // Identifier containing default prefix
        r#"
fn main() {
    default_val := 10
    fmt.println("{default_val}")
}
"#,
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "expected Accept for valid code:\n{src}\ngot {r:?}"
        );
        let errs = errors(&r);
        assert!(errs.is_empty(), "expected no errors for valid code, got: {errs:?}");
    }
}
