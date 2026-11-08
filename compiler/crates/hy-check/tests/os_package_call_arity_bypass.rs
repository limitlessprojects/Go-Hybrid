//! Regression tests for Wave 20:
//! Curated foreign package function calls (`os.*`) must enforce call arity
//! constraints and non-callable member checks during `hyb check` (HYB-E1002, Row 13 BUILDABLE).
//!
//! Without arity checks on these foreign package calls, invocations with too few or too many arguments,
//! or invocations on non-callable package types/constants/variables (`os.file`, `os.process`, `os.dev_null`, `os.err_exist`),
//! passed `hyb check` with 0 diagnostics and subsequently failed downstream `go build`.

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
    assert!(
        cs.is_empty(),
        "expected accept for {:?}, got errors: {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Accept, "verdict for {:?}", src);
}

#[test]
fn test_os_package_call_too_few_arguments_rejected() {
    // os.exit takes 1 argument, called with 0
    rejects_e1002(
        r#"
package main

import "os"

fn test() {
    os.exit()
}
"#,
    );

    // os.getenv takes 1 argument, called with 0
    rejects_e1002(
        r#"
package main

import "os"

fn test() {
    _ = os.getenv()
}
"#,
    );

    // os.setenv takes 2 arguments, called with 1
    rejects_e1002(
        r#"
package main

import "os"

fn test() {
    _ = os.setenv("KEY")
}
"#,
    );

    // os.mkdir takes 2 arguments, called with 1
    rejects_e1002(
        r#"
package main

import "os"

fn test() {
    _ = os.mkdir("path")
}
"#,
    );

    // os.write_file takes 3 arguments, called with 2
    rejects_e1002(
        r#"
package main

import "os"

fn test(data []byte) {
    _ = os.write_file("file.txt", data)
}
"#,
    );
}

#[test]
fn test_os_package_call_too_many_arguments_rejected() {
    // os.clearenv takes 0 arguments, called with 1
    rejects_e1002(
        r#"
package main

import "os"

fn test() {
    os.clearenv("extra")
}
"#,
    );

    // os.getpid takes 0 arguments, called with 1
    rejects_e1002(
        r#"
package main

import "os"

fn test() {
    _ = os.getpid(123)
}
"#,
    );

    // os.exit takes 1 argument, called with 2
    rejects_e1002(
        r#"
package main

import "os"

fn test() {
    os.exit(1, 2)
}
"#,
    );

    // os.getenv takes 1 argument, called with 2
    rejects_e1002(
        r#"
package main

import "os"

fn test() {
    _ = os.getenv("A", "B")
}
"#,
    );
}

#[test]
fn test_os_non_callable_members_rejected() {
    // os.dev_null is a string constant/variable, not a function
    rejects_e1002(
        r#"
package main

import "os"

fn test() {
    _ = os.dev_null()
}
"#,
    );

    // os.err_exist is an error variable, not a function
    rejects_e1002(
        r#"
package main

import "os"

fn test() {
    _ = os.err_exist()
}
"#,
    );

    // os.err_not_exist is an error variable, not a function
    rejects_e1002(
        r#"
package main

import "os"

fn test() {
    _ = os.err_not_exist()
}
"#,
    );

    // os.file is a struct type, not a function
    rejects_e1002(
        r#"
package main

import "os"

fn test() {
    _ = os.file()
}
"#,
    );

    // os.process is a struct type, not a function
    rejects_e1002(
        r#"
package main

import "os"

fn test() {
    _ = os.process()
}
"#,
    );

    // os.o_rdonly is an int constant, not a function
    rejects_e1002(
        r#"
package main

import "os"

fn test() {
    _ = os.o_rdonly()
}
"#,
    );
}

#[test]
fn test_os_variadic_spread_on_non_variadic_rejected() {
    // os.exit is non-variadic; spread `...` must be rejected
    rejects_e1002(
        r#"
package main

import "os"

fn test(args []int) {
    os.exit(args...)
}
"#,
    );

    // os.getenv is non-variadic; spread `...` must be rejected
    rejects_e1002(
        r#"
package main

import "os"

fn test(args []string) {
    _ = os.getenv(args...)
}
"#,
    );
}

#[test]
fn test_os_positive_controls_accepted() {
    accepts(
        r#"
package main

import "os"

fn test() {
    os.clearenv()
    _ = os.getpid()
    _ = os.getppid()
    _ = os.temp_dir()
    _ = os.getenv("HOME")
    os.exit(0)
}
"#,
    );
}
