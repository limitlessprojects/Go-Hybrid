//! issue_3863_special_member_lowerings.rs
//! Regression tests for #3863: validate arity and operands of special member lowerings
//! (time duration constructors, os variables, hex.encode) under Row 13 (BUILDABLE).

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("test.hy", src)
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

fn rejects_e1002(src: &str, expected_substring: &str) {
    let r = check(src);
    let codes = error_codes(&r);
    assert!(
        codes.contains(&"HYB-E1002".to_string()),
        "must emit HYB-E1002, got: {:?} for\n{}",
        codes,
        src
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.message.contains(expected_substring)),
        "diagnostic message must contain {:?}, got: {:?}",
        expected_substring,
        r.diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

fn accepts(src: &str) {
    let r = check(src);
    let codes = error_codes(&r);
    assert!(
        codes.is_empty(),
        "must accept with 0 errors, got: {:?} for\n{}",
        codes,
        src
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn test_duration_constructor_arity_rejection() {
    let repro_zero_args = r#"
import "time"

fn main() {
    d := time.milliseconds()
    _ = d
}
"#;
    rejects_e1002(repro_zero_args, "`time.milliseconds` takes exactly 1 argument `(n)`, got 0");

    let repro_multi_args = r#"
import "time"

fn main() {
    d := time.seconds(1, 2)
    _ = d
}
"#;
    rejects_e1002(repro_multi_args, "`time.seconds` takes exactly 1 argument `(n)`, got 2");
}

#[test]
fn test_duration_constructor_non_numeric_rejection() {
    let repro_string = r#"
import "time"

fn main() {
    d := time.milliseconds("100")
    _ = d
}
"#;
    rejects_e1002(repro_string, "`time.milliseconds` requires a numeric argument, found non-numeric literal");

    let repro_bool = r#"
import "time"

fn main() {
    d := time.seconds(true)
    _ = d
}
"#;
    rejects_e1002(repro_bool, "`time.seconds` requires a numeric argument, found non-numeric literal");

    let repro_nil = r#"
import "time"

fn main() {
    d := time.minutes(nil)
    _ = d
}
"#;
    rejects_e1002(repro_nil, "`time.minutes` requires a numeric argument, found non-numeric literal");
}

#[test]
fn test_os_variable_calls_with_arguments_rejection() {
    let repro_args = r#"
import "os"

fn main() {
    a := os.args(1)
    _ = a
}
"#;
    rejects_e1002(repro_args, "`os.args` takes no arguments, got 1");

    let repro_stdout = r#"
import "os"

fn main() {
    out := os.stdout("hello")
    _ = out
}
"#;
    rejects_e1002(repro_stdout, "`os.stdout` takes no arguments, got 1");
}

#[test]
fn test_hex_encode_arity_rejection() {
    let repro_zero = r#"
import "encoding/hex"

fn main() {
    h := hex.encode()
    _ = h
}
"#;
    rejects_e1002(repro_zero, "`hex.encode` takes exactly 1 slice argument `(src)`, got 0");

    let repro_two = r#"
import "encoding/hex"

fn main() {
    b1 := "hello"
    b2 := "world"
    h := hex.encode(b1, b2)
    _ = h
}
"#;
    rejects_e1002(repro_two, "`hex.encode` takes exactly 1 slice argument `(src)`, got 2");
}

#[test]
fn test_valid_special_members_stay_accepted() {
    let valid_src = r#"
import "time"
import "os"
import "encoding/hex"

fn main() {
    d1 := time.milliseconds(100)
    d2 := time.seconds(5)
    d3 := time.minutes(10)
    d4 := time.hours(2)
    a := os.args
    out := os.stdout
    h := hex.encode("test")
    _ = d1
    _ = d2
    _ = d3
    _ = d4
    _ = a
    _ = out
    _ = h
}
"#;
    accepts(valid_src);
}
