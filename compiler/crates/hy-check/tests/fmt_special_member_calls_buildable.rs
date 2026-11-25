//! fmt_special_member_calls_buildable.rs — regression tests for fmt printf-family member calls
//! (HYB-E1002 arity and format string operand validation, Row 13 BUILDABLE).
//!
//! Go standard library contract:
//! - `fmt.Printf(format string, a ...any)` requires at least 1 argument (format string).
//! - `fmt.Sprintf(format string, a ...any)` requires at least 1 argument (format string).
//! - `fmt.Errorf(format string, a ...any)` requires at least 1 argument (format string).
//! - `fmt.Fprintf(w io.Writer, format string, a ...any)` requires at least 2 arguments (w, format).
//! - `fmt.Fprint(w io.Writer, a ...any)` and `fmt.Fprintln` require at least 1 argument (w).
//! - In Go-Hybrid, `fmt.eprintf` lowers to `fmt.Fprintf(os.Stderr, format, a...)` and requires format string.
//!
//! When callers omit arguments (e.g. `fmt.printf()`) or pass non-string literals (e.g. `fmt.sprintf(123)`),
//! emitted Go code fails `go build` with "not enough arguments in call" or "cannot use ... as string value".
//! `hy-check` must reject these calls with `HYB-E1002` under GUARANTEES.md Row 13.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects_e1002(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for:\n{}\ngot: {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.is_empty(),
        "expected Accept with 0 errors for:\n{}\ngot: {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept for:\n{}", src);
}

// ---------------------------------------------------------------------------
// Arity violations (missing format string or writer)
// ---------------------------------------------------------------------------

#[test]
fn test_fmt_printf_zero_args_rejects() {
    rejects_e1002("import \"fmt\"\nfn main() {\n    fmt.printf()\n}\n");
}

#[test]
fn test_fmt_sprintf_zero_args_rejects() {
    rejects_e1002("import \"fmt\"\nfn main() {\n    s := fmt.sprintf()\n    _ = s\n}\n");
}

#[test]
fn test_fmt_eprintf_zero_args_rejects() {
    rejects_e1002("import \"fmt\"\nfn main() {\n    fmt.eprintf()\n}\n");
}

#[test]
fn test_fmt_errorf_zero_args_rejects() {
    rejects_e1002("import \"fmt\"\nfn main() {\n    err := fmt.errorf()\n    _ = err\n}\n");
}

#[test]
fn test_fmt_fprintf_under_two_args_rejects() {
    rejects_e1002("import \"fmt\"\nimport \"os\"\nfn main() {\n    fmt.fprintf()\n}\n");
    rejects_e1002("import \"fmt\"\nimport \"os\"\nfn main() {\n    fmt.fprintf(os.stdout)\n}\n");
}

#[test]
fn test_fmt_fprint_zero_args_rejects() {
    rejects_e1002("import \"fmt\"\nfn main() {\n    fmt.fprint()\n}\n");
    rejects_e1002("import \"fmt\"\nfn main() {\n    fmt.fprintln()\n}\n");
}

// ---------------------------------------------------------------------------
// Non-string literal format operands
// ---------------------------------------------------------------------------

#[test]
fn test_fmt_printf_non_string_literal_rejects() {
    rejects_e1002("import \"fmt\"\nfn main() {\n    fmt.printf(123)\n}\n");
    rejects_e1002("import \"fmt\"\nfn main() {\n    fmt.printf(true)\n}\n");
    rejects_e1002("import \"fmt\"\nfn main() {\n    fmt.printf(nil)\n}\n");
}

#[test]
fn test_fmt_sprintf_non_string_literal_rejects() {
    rejects_e1002("import \"fmt\"\nfn main() {\n    s := fmt.sprintf(42)\n    _ = s\n}\n");
    rejects_e1002("import \"fmt\"\nfn main() {\n    s := fmt.sprintf(false)\n    _ = s\n}\n");
}

#[test]
fn test_fmt_eprintf_non_string_literal_rejects() {
    rejects_e1002("import \"fmt\"\nfn main() {\n    fmt.eprintf(999)\n}\n");
}

#[test]
fn test_fmt_errorf_non_string_literal_rejects() {
    rejects_e1002("import \"fmt\"\nfn main() {\n    err := fmt.errorf(nil)\n    _ = err\n}\n");
}

#[test]
fn test_fmt_fprintf_non_string_format_literal_rejects() {
    rejects_e1002("import \"fmt\"\nimport \"os\"\nfn main() {\n    fmt.fprintf(os.stdout, 123)\n}\n");
    rejects_e1002("import \"fmt\"\nimport \"os\"\nfn main() {\n    fmt.fprintf(os.stderr, true)\n}\n");
}

// ---------------------------------------------------------------------------
// Positive controls: valid format calls must keep accepting
// ---------------------------------------------------------------------------

#[test]
fn test_fmt_valid_calls_accept() {
    let src = r#"
import "fmt"
import "os"

fn main() {
    fmt.printf("value: %d\n", 42)
    s := fmt.sprintf("hello %s", "world")
    fmt.eprintf("error: %s\n", "failed")
    err := fmt.errorf("not found: %d", 404)
    fmt.fprintf(os.stdout, "status=%v\n", true)
    fmt.fprint(os.stdout, "ok")
    fmt.fprintln(os.stdout, "done")
    _ = s
    _ = err
}
"#;
    accepts(src);
}

#[test]
fn test_fmt_shadowed_package_keeps_user_semantics() {
    let src = r#"
struct my_fmt {
    val int
}

fn (f my_fmt) printf() int {
    return f.val
}

fn main() {
    fmt := my_fmt{val: 10}
    _ = fmt.printf()
}
"#;
    accepts(src);
}

// ---------------------------------------------------------------------------
// Known non-string format expressions (issue addendum: #4087 only rejected
// non-string *literals* — `fmt.printf(x)` with `x := 42` passed check and
// died in `go build` because the emitted `fmt.Printf(x)` does not compile).
// ---------------------------------------------------------------------------

#[test]
fn test_fmt_printf_int_local_rejects() {
    rejects_e1002("import \"fmt\"\nfn main() {\n    x := 42\n    fmt.printf(x)\n}\n");
}

#[test]
fn test_fmt_printf_int_binop_rejects() {
    rejects_e1002("import \"fmt\"\nfn main() {\n    fmt.printf(42 * 1)\n}\n");
}

#[test]
fn test_fmt_printf_float_binop_rejects() {
    rejects_e1002("import \"fmt\"\nfn main() {\n    fmt.printf(1.5 + 2.5)\n}\n");
}

#[test]
fn test_fmt_sprintf_int_local_rejects() {
    rejects_e1002("import \"fmt\"\nfn main() {\n    x := 42\n    s := fmt.sprintf(x)\n    _ = s\n}\n");
}

#[test]
fn test_fmt_eprintf_int_local_rejects() {
    rejects_e1002("import \"fmt\"\nfn main() {\n    x := 42\n    fmt.eprintf(x)\n}\n");
}

#[test]
fn test_fmt_errorf_int_local_rejects() {
    rejects_e1002("import \"fmt\"\nfn main() {\n    x := 42\n    err := fmt.errorf(x)\n    _ = err\n}\n");
}

#[test]
fn test_fmt_fprintf_int_local_format_rejects() {
    let src = r#"
import "fmt"
import "os"

fn main() {
    x := 42
    fmt.fprintf(os.stdout, x)
}
"#;
    rejects_e1002(src);
}

#[test]
fn test_fmt_known_kind_accepts_stay_accepted() {
    let src = r#"
import "fmt"

fn main() {
    s := "hello"
    fmt.printf(s)
    fmt.printf("a" + "b")
    a := any(1)
    fmt.printf(a)
    f := "fmt %s"
    v := fmt.sprintf(f, 1)
    _ = v
}
"#;
    accepts(src);
}
