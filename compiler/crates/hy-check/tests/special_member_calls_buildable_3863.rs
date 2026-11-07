//! Regression tests for #3863:
//! Special foreign member lowerings in `hy-emit` (`MemberSpecial`):
//! 1. `time.milliseconds`, `time.seconds`, `time.minutes`, `time.hours`
//!    require exactly 1 numeric argument. Arity mismatches and non-numeric
//!    literals must be rejected with `HYB-E1002` (Row 13 BUILDABLE).
//! 2. `os.args`, `os.stdout`, `os.stderr`, `os.stdin` are package-level
//!    variables that lower to Go vars (`os.Args`, `os.Stdout`, etc.) and take
//!    0 arguments. Calling them with arguments must reject with `HYB-E1002`.
//! 3. `hex.encode` lowers to Go `hex.EncodeToString(src []byte)` and takes
//!    exactly 1 argument. Arity mismatches must reject with `HYB-E1002`.

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

fn rejects_e1002(src: &str) {
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
    assert_eq!(r.verdict, Verdict::Accept, "verdict for {:?}", src);
}

#[test]
fn test_time_duration_constructors_zero_args_reject() {
    rejects_e1002("import \"time\"\nfn main() {\n    d := time.milliseconds()\n    _ = d\n}\n");
    rejects_e1002("import \"time\"\nfn main() {\n    d := time.seconds()\n    _ = d\n}\n");
    rejects_e1002("import \"time\"\nfn main() {\n    d := time.minutes()\n    _ = d\n}\n");
    rejects_e1002("import \"time\"\nfn main() {\n    d := time.hours()\n    _ = d\n}\n");
}

#[test]
fn test_time_duration_constructors_multiple_args_reject() {
    rejects_e1002("import \"time\"\nfn main() {\n    d := time.milliseconds(1, 2)\n    _ = d\n}\n");
    rejects_e1002("import \"time\"\nfn main() {\n    d := time.seconds(1, 2)\n    _ = d\n}\n");
    rejects_e1002("import \"time\"\nfn main() {\n    d := time.minutes(1, 2)\n    _ = d\n}\n");
    rejects_e1002("import \"time\"\nfn main() {\n    d := time.hours(1, 2)\n    _ = d\n}\n");
}

#[test]
fn test_time_duration_constructors_non_numeric_literal_reject() {
    rejects_e1002("import \"time\"\nfn main() {\n    d := time.milliseconds(\"100\")\n    _ = d\n}\n");
    rejects_e1002("import \"time\"\nfn main() {\n    d := time.seconds(true)\n    _ = d\n}\n");
    rejects_e1002("import \"time\"\nfn main() {\n    d := time.minutes(nil)\n    _ = d\n}\n");
}

#[test]
fn test_os_variable_calls_with_arguments_reject() {
    rejects_e1002("import \"os\"\nfn main() {\n    a := os.args(1)\n    _ = a\n}\n");
    rejects_e1002("import \"os\"\nfn main() {\n    s := os.stdout(\"hello\")\n    _ = s\n}\n");
    rejects_e1002("import \"os\"\nfn main() {\n    e := os.stderr(1, 2)\n    _ = e\n}\n");
    rejects_e1002("import \"os\"\nfn main() {\n    i := os.stdin(true)\n    _ = i\n}\n");
}

#[test]
fn test_hex_encode_arity_rejects() {
    rejects_e1002("import \"encoding/hex\"\nfn main() {\n    s := hex.encode()\n    _ = s\n}\n");
    rejects_e1002("import \"encoding/hex\"\nfn main() {\n    b := \"hi\".bytes()\n    s := hex.encode(b, b)\n    _ = s\n}\n");
}

#[test]
fn test_special_member_calls_positive_controls_accept() {
    accepts("import \"time\"\nfn main() {\n    d := time.milliseconds(500)\n    s := time.seconds(10)\n    m := time.minutes(1)\n    h := time.hours(24)\n    _ = d\n    _ = s\n    _ = m\n    _ = h\n}\n");
    accepts("import \"os\"\nfn main() {\n    a := os.args()\n    s := os.stdout()\n    e := os.stderr()\n    i := os.stdin()\n    _ = a\n    _ = s\n    _ = e\n    _ = i\n}\n");
    accepts("import \"encoding/hex\"\nfn main() {\n    b := \"hi\".bytes()\n    s := hex.encode(b)\n    _ = s\n}\n");
}