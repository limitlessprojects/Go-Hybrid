//! Regression tests for #4124:
//! `io` and `os` package functions must enforce call arity constraints, and package
//! variables/constants must not be callable as functions during `hyb check` (HYB-E1002, Row 13 BUILDABLE).
//!
//! Without call validation, invocations of non-callable package values (like `io.eof()` or `os.stdout()`)
//! and calls with invalid arities passed `hyb check` with 0 diagnostics and subsequently failed `go build`.

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
fn test_io_non_callable_rejection() {
    // io.eof is an error variable, not a function
    rejects_e1002(
        r#"
package main

import "io"

fn test() {
    io.eof()
}
"#,
    );

    // io.discard is an io.Writer variable, not a function
    rejects_e1002(
        r#"
package main

import "io"

fn test() {
    io.discard()
}
"#,
    );

    // io.err_unexpected_eof is an error variable
    rejects_e1002(
        r#"
package main

import "io"

fn test() {
    io.err_unexpected_eof()
}
"#,
    );
}

#[test]
fn test_os_non_callable_rejection() {
    // os.stdin is an *os.File variable, not a function
    rejects_e1002(
        r#"
package main

import "os"

fn test() {
    os.stdin()
}
"#,
    );

    // os.stdout is an *os.File variable
    rejects_e1002(
        r#"
package main

import "os"

fn test() {
    os.stdout()
}
"#,
    );

    // os.dev_null is a constant
    rejects_e1002(
        r#"
package main

import "os"

fn test() {
    os.dev_null()
}
"#,
    );

    // os.args is a slice variable
    rejects_e1002(
        r#"
package main

import "os"

fn test() {
    os.args()
}
"#,
    );
}

#[test]
fn test_io_arity_rejection() {
    // io.copy requires 2 arguments
    rejects_e1002(
        r#"
package main

import "io"

fn test() {
    io.copy()
}
"#,
    );

    rejects_e1002(
        r#"
package main

import "io"

fn test(w io.writer) {
    io.copy(w)
}
"#,
    );

    // io.pipe takes 0 arguments
    rejects_e1002(
        r#"
package main

import "io"

fn test() {
    io.pipe(1)
}
"#,
    );

    // io.read_all takes 1 argument
    rejects_e1002(
        r#"
package main

import "io"

fn test() {
    io.read_all()
}
"#,
    );
}

#[test]
fn test_os_arity_rejection() {
    // os.open requires 1 argument
    rejects_e1002(
        r#"
package main

import "os"

fn test() {
    os.open()
}
"#,
    );

    rejects_e1002(
        r#"
package main

import "os"

fn test() {
    os.open("a", "b")
}
"#,
    );

    // os.exit requires 1 argument
    rejects_e1002(
        r#"
package main

import "os"

fn test() {
    os.exit()
}
"#,
    );

    rejects_e1002(
        r#"
package main

import "os"

fn test() {
    os.exit(1, 2)
}
"#,
    );

    // os.getwd takes 0 arguments
    rejects_e1002(
        r#"
package main

import "os"

fn test() {
    os.getwd(1)
}
"#,
    );
}

#[test]
fn test_valid_io_and_os_package_calls_accepted() {
    accepts(
        r#"
package main

import "io"
import "os"

fn test(r io.reader, w io.writer) {
    io.pipe()
    io.read_all(r)
    io.copy(w, r)
    os.getwd()
    os.open("test.txt")
    os.exit(0)
}
"#,
    );
}
