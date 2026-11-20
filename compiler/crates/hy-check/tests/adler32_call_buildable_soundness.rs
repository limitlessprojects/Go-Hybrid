//! adler32_call_buildable_soundness.rs — regression tests for hash/adler32 call arity,
//! non-callable constant rejection, and variadic spread enforcement (fixes #4317).

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
        "expected zero errors for:\n{}\ngot: {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept for:\n{}", src);
}

#[test]
fn test_adler32_new_nullary_arity_rejected() {
    // adler32.new takes exactly 0 arguments
    rejects_e1002(
        r#"package main
import "hash/adler32"

fn main() {
    _ = adler32.new(1)
}
"#,
    );
    rejects_e1002(
        r#"package main
import "hash/adler32"

fn main() {
    _ = adler32.new(1, 2)
}
"#,
    );
}

#[test]
fn test_adler32_checksum_unary_arity_rejected() {
    // adler32.checksum takes exactly 1 argument (data []byte)
    rejects_e1002(
        r#"package main
import "hash/adler32"

fn main() {
    _ = adler32.checksum()
}
"#,
    );
    rejects_e1002(
        r#"package main
import "hash/adler32"

fn test_it(b any) {
    _ = adler32.checksum(b, 1)
}
"#,
    );
    rejects_e1002(
        r#"package main
import "hash/adler32"

fn test_it(b any) {
    _ = adler32.checksum(b, 1, 2)
}
"#,
    );
}

#[test]
fn test_adler32_constants_rejected_as_call_targets() {
    // adler32.size is a constant and cannot be invoked as a function
    rejects_e1002(
        r#"package main
import "hash/adler32"

fn main() {
    _ = adler32.size()
}
"#,
    );
}

#[test]
fn test_adler32_variadic_spread_rejected() {
    // non-variadic adler32 calls reject spread arguments
    rejects_e1002(
        r#"package main
import "hash/adler32"

fn test_it(xs []any) {
    _ = adler32.checksum(xs...)
}
"#,
    );
}

#[test]
fn test_adler32_valid_calls_and_references_accept() {
    accepts(
        r#"package main
import "hash/adler32"

fn run(data []byte) {
    h := adler32.new()
    _ = h
    cs := adler32.checksum(data)
    _ = cs
    sz := adler32.size
    _ = sz
}

fn main() {
}
"#,
    );
}
