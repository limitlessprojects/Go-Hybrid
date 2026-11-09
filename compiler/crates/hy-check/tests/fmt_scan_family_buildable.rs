//! Regression tests for fmt scan-family member calls (Row 13 BUILDABLE):
//!
//! Calls to `fmt.scanf`, `fmt.sscanf`, `fmt.fscanf`, `fmt.sscan`, `fmt.sscanln`,
//! `fmt.fscan`, and `fmt.fscanln` without required arguments or with non-string
//! format/input string literals cause downstream `go build` compilation errors:
//! - "not enough arguments in call to fmt.Scanf/Sscanf/Fscanf/Sscan/Sscanln/Fscan/Fscanln"
//! - "cannot use ... (untyped int constant) as string value in argument to fmt...."
//!
//! `hy-check` must reject these invalid calls with `HYB-E1002`.

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
// Arity violations (missing format string, input string, or reader)
// ---------------------------------------------------------------------------

#[test]
fn test_fmt_scanf_zero_args_rejects() {
    rejects_e1002("import \"fmt\"\nfn main() {\n    fmt.scanf()\n}\n");
}

#[test]
fn test_fmt_sscanf_under_two_args_rejects() {
    rejects_e1002("import \"fmt\"\nfn main() {\n    fmt.sscanf()\n}\n");
    rejects_e1002("import \"fmt\"\nfn main() {\n    fmt.sscanf(\"123\")\n}\n");
}

#[test]
fn test_fmt_fscanf_under_two_args_rejects() {
    rejects_e1002("import \"fmt\"\nimport \"os\"\nfn main() {\n    fmt.fscanf()\n}\n");
    rejects_e1002("import \"fmt\"\nimport \"os\"\nfn main() {\n    fmt.fscanf(os.stdin)\n}\n");
}

#[test]
fn test_fmt_sscan_zero_args_rejects() {
    rejects_e1002("import \"fmt\"\nfn main() {\n    fmt.sscan()\n}\n");
}

#[test]
fn test_fmt_sscanln_zero_args_rejects() {
    rejects_e1002("import \"fmt\"\nfn main() {\n    fmt.sscanln()\n}\n");
}

#[test]
fn test_fmt_fscan_zero_args_rejects() {
    rejects_e1002("import \"fmt\"\nfn main() {\n    fmt.fscan()\n}\n");
}

#[test]
fn test_fmt_fscanln_zero_args_rejects() {
    rejects_e1002("import \"fmt\"\nfn main() {\n    fmt.fscanln()\n}\n");
}

// ---------------------------------------------------------------------------
// Non-string literal operand violations
// ---------------------------------------------------------------------------

#[test]
fn test_fmt_scanf_non_string_literal_rejects() {
    rejects_e1002("import \"fmt\"\nfn main() {\n    fmt.scanf(123)\n}\n");
    rejects_e1002("import \"fmt\"\nfn main() {\n    fmt.scanf(true)\n}\n");
    rejects_e1002("import \"fmt\"\nfn main() {\n    fmt.scanf(nil)\n}\n");
}

#[test]
fn test_fmt_sscanf_non_string_literals_rejects() {
    rejects_e1002("import \"fmt\"\nfn main() {\n    fmt.sscanf(123, \"%d\")\n}\n");
    rejects_e1002("import \"fmt\"\nfn main() {\n    fmt.sscanf(\"123\", 456)\n}\n");
    rejects_e1002("import \"fmt\"\nfn main() {\n    fmt.sscanf(true, false)\n}\n");
}

#[test]
fn test_fmt_fscanf_non_string_format_literal_rejects() {
    rejects_e1002("import \"fmt\"\nimport \"os\"\nfn main() {\n    fmt.fscanf(os.stdin, 123)\n}\n");
    rejects_e1002("import \"fmt\"\nimport \"os\"\nfn main() {\n    fmt.fscanf(os.stdin, true)\n}\n");
}

#[test]
fn test_fmt_sscan_non_string_literal_rejects() {
    rejects_e1002("import \"fmt\"\nfn main() {\n    fmt.sscan(123)\n}\n");
    rejects_e1002("import \"fmt\"\nfn main() {\n    fmt.sscan(true)\n}\n");
}

#[test]
fn test_fmt_sscanln_non_string_literal_rejects() {
    rejects_e1002("import \"fmt\"\nfn main() {\n    fmt.sscanln(123)\n}\n");
    rejects_e1002("import \"fmt\"\nfn main() {\n    fmt.sscanln(false)\n}\n");
}

// ---------------------------------------------------------------------------
// Non-string expression operand violations (#4265)
// ---------------------------------------------------------------------------

#[test]
fn test_fmt_scanf_non_string_local_rejects() {
    rejects_e1002("import \"fmt\"\nfn main() {\n    x := 42\n    fmt.scanf(x)\n}\n");
    rejects_e1002("import \"fmt\"\nfn main() {\n    b := true\n    fmt.scanf(b)\n}\n");
    rejects_e1002("import \"fmt\"\nfn main() {\n    fmt.scanf(40 + 2)\n}\n");
}

#[test]
fn test_fmt_sscanf_non_string_input_local_rejects() {
    rejects_e1002("import \"fmt\"\nfn main() {\n    x := 42\n    fmt.sscanf(x, \"%d\")\n}\n");
    rejects_e1002("import \"fmt\"\nfn main() {\n    fmt.sscanf(10 + 20, \"%d\")\n}\n");
}

#[test]
fn test_fmt_sscanf_non_string_format_local_rejects() {
    rejects_e1002("import \"fmt\"\nfn main() {\n    x := 42\n    fmt.sscanf(\"123\", x)\n}\n");
    rejects_e1002("import \"fmt\"\nfn main() {\n    fmt.sscanf(\"123\", 10 * 2)\n}\n");
}

#[test]
fn test_fmt_fscanf_non_string_format_local_rejects() {
    rejects_e1002("import \"fmt\"\nimport \"os\"\nfn main() {\n    x := 42\n    fmt.fscanf(os.stdin, x)\n}\n");
    rejects_e1002("import \"fmt\"\nimport \"os\"\nfn main() {\n    fmt.fscanf(os.stdin, 50 - 8)\n}\n");
}

#[test]
fn test_fmt_sscan_non_string_local_rejects() {
    rejects_e1002("import \"fmt\"\nfn main() {\n    x := 42\n    fmt.sscan(x)\n}\n");
    rejects_e1002("import \"fmt\"\nfn main() {\n    fmt.sscan(30 + 12)\n}\n");
}

#[test]
fn test_fmt_sscanln_non_string_local_rejects() {
    rejects_e1002("import \"fmt\"\nfn main() {\n    x := 42\n    fmt.sscanln(x)\n}\n");
    rejects_e1002("import \"fmt\"\nfn main() {\n    fmt.sscanln(30 + 12)\n}\n");
}

#[test]
fn test_fmt_scan_with_string_variables_accept() {
    let src = r#"
import "fmt"
import "os"

fn main() {
    var v int
    fmt_str := "%d"
    input_str := "42"
    _, _ = fmt.scanf(fmt_str, &v)
    _, _ = fmt.sscanf(input_str, fmt_str, &v)
    _, _ = fmt.fscanf(os.stdin, fmt_str, &v)
    _, _ = fmt.sscan(input_str, &v)
    _, _ = fmt.sscanln(input_str, &v)
}
"#;
    accepts(src);
}

// ---------------------------------------------------------------------------
// Positive controls: valid scan calls must keep accepting
// ---------------------------------------------------------------------------

#[test]
fn test_fmt_scan_valid_calls_accept() {
    let src = r#"
import "fmt"
import "os"

fn main() {
    var v int
    _, _ = fmt.scanf("%d", &v)
    _, _ = fmt.sscanf("42", "%d", &v)
    _, _ = fmt.fscanf(os.stdin, "%d", &v)
    _, _ = fmt.sscan("42", &v)
    _, _ = fmt.sscanln("42", &v)
    _, _ = fmt.fscan(os.stdin, &v)
    _, _ = fmt.fscanln(os.stdin, &v)
}
"#;
    accepts(src);
}

#[test]
fn test_fmt_scan_shadowed_package_keeps_user_semantics() {
    let src = r#"
struct my_scanner {
    val int
}

fn (s my_scanner) scanf() int {
    return s.val
}

fn (s my_scanner) sscanf(x int) int {
    return s.val + x
}

fn main() {
    fmt := my_scanner{val: 10}
    _ = fmt.scanf()
    _ = fmt.sscanf(5)
}
"#;
    accepts(src);
}
