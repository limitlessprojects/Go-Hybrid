//! fmt_append_arity_and_format_operand_bypass.rs: regression tests for `fmt.append`,
//! `fmt.appendf`, and `fmt.appendln` member calls.
//!
//! Under GUARANTEES.md Row 13 (BUILDABLE), programs accepted by `hy-check` must emit
//! Go code that compiles under `go build`.
//!
//! Standard Go §fmt:
//! - `fmt.Append(b []byte, a ...any) []byte`
//! - `fmt.Appendf(b []byte, format string, a ...any) []byte`
//! - `fmt.Appendln(b []byte, a ...any) []byte`
//!
//! Calls with missing arguments (e.g. `fmt.append()`, `fmt.appendln()`, `fmt.appendf()`,
//! `fmt.appendf(b)`) or non-string format arguments (e.g. `fmt.appendf(b, 123)`) or
//! non-slice destination literals (e.g. `fmt.append(123)`) must be rejected with `HYB-E1002`.

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
}

fn accepts(src: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected Accept for {:?}, got {:?}",
        src,
        r.diagnostics
    );
}

#[test]
fn rejects_fmt_append_zero_args() {
    rejects(
        "import \"fmt\"\n\
         fn main() {\n\
             _ = fmt.append()\n\
         }",
    );
}

#[test]
fn rejects_fmt_append_non_slice_literal() {
    rejects(
        "import \"fmt\"\n\
         fn main() {\n\
             _ = fmt.append(123, \"val\")\n\
         }",
    );
    rejects(
        "import \"fmt\"\n\
         fn main() {\n\
             _ = fmt.append(\"dest\", 123)\n\
         }",
    );
}

#[test]
fn rejects_fmt_appendln_zero_args() {
    rejects(
        "import \"fmt\"\n\
         fn main() {\n\
             _ = fmt.appendln()\n\
         }",
    );
}

#[test]
fn rejects_fmt_appendln_non_slice_literal() {
    rejects(
        "import \"fmt\"\n\
         fn main() {\n\
             _ = fmt.appendln(true, 123)\n\
         }",
    );
}

#[test]
fn rejects_fmt_appendf_zero_args() {
    rejects(
        "import \"fmt\"\n\
         fn main() {\n\
             _ = fmt.appendf()\n\
         }",
    );
}

#[test]
fn rejects_fmt_appendf_one_arg() {
    rejects(
        "import \"fmt\"\n\
         fn main() {\n\
             b := []byte{}\n\
             _ = fmt.appendf(b)\n\
         }",
    );
}

#[test]
fn rejects_fmt_appendf_non_string_format_literal() {
    rejects(
        "import \"fmt\"\n\
         fn main() {\n\
             b := []byte{}\n\
             _ = fmt.appendf(b, 123)\n\
         }",
    );
    rejects(
        "import \"fmt\"\n\
         fn main() {\n\
             b := []byte{}\n\
             _ = fmt.appendf(b, nil)\n\
         }",
    );
}

#[test]
fn accepts_valid_fmt_append_calls() {
    accepts(
        "import \"fmt\"\n\
         fn main() {\n\
             b := []byte{}\n\
             b = fmt.append(b, \"hello\", 123)\n\
             b = fmt.appendln(b, \"world\")\n\
             b = fmt.appendf(b, \"%s: %d\", \"count\", 42)\n\
             _ = b\n\
         }",
    );
}
