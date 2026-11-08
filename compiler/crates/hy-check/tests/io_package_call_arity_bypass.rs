//! Regression tests for Wave 21:
//! Curated foreign package function calls (`io.*`) must enforce call arity
//! constraints and non-callable member checks during `hyb check` (HYB-E1002, Row 13 BUILDABLE).
//!
//! Without arity checks on these foreign package calls, invocations with too few or too many arguments,
//! or invocations on non-callable package types/variables/constants (`io.eof`, `io.err_closed_pipe`, `io.discard`, `io.reader`, `io.writer`),
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
fn test_io_package_call_too_few_arguments_rejected() {
    // io.read_all takes 1 argument, called with 0
    rejects_e1002(
        r#"
package main

import "io"

fn test() {
    _, _ = io.read_all()
}
"#,
    );

    // io.copy takes 2 arguments, called with 1
    rejects_e1002(
        r#"
package main

import "io"

fn test(w io.writer) {
    _, _ = io.copy(w)
}
"#,
    );

    // io.limit_reader takes 2 arguments, called with 1
    rejects_e1002(
        r#"
package main

import "io"

fn test(r io.reader) {
    _ = io.limit_reader(r)
}
"#,
    );

    // io.read_full takes 2 arguments, called with 1
    rejects_e1002(
        r#"
package main

import "io"

fn test(r io.reader) {
    _, _ = io.read_full(r)
}
"#,
    );

    // io.copy_n takes 3 arguments, called with 2
    rejects_e1002(
        r#"
package main

import "io"

fn test(w io.writer, r io.reader) {
    _, _ = io.copy_n(w, r)
}
"#,
    );
}

#[test]
fn test_io_package_call_too_many_arguments_rejected() {
    // io.pipe takes 0 arguments, called with 1
    rejects_e1002(
        r#"
package main

import "io"

fn test() {
    _, _ = io.pipe("extra")
}
"#,
    );

    // io.read_all takes 1 argument, called with 2
    rejects_e1002(
        r#"
package main

import "io"

fn test(r io.reader) {
    _, _ = io.read_all(r, "extra")
}
"#,
    );

    // io.copy takes 2 arguments, called with 3
    rejects_e1002(
        r#"
package main

import "io"

fn test(w io.writer, r io.reader) {
    _, _ = io.copy(w, r, "extra")
}
"#,
    );
}

#[test]
fn test_io_non_callable_members_rejected() {
    // io.eof is an error variable, not a function
    rejects_e1002(
        r#"
package main

import "io"

fn test() {
    _ = io.eof()
}
"#,
    );

    // io.err_closed_pipe is an error variable, not a function
    rejects_e1002(
        r#"
package main

import "io"

fn test() {
    _ = io.err_closed_pipe()
}
"#,
    );

    // io.discard is a package variable, not a function
    rejects_e1002(
        r#"
package main

import "io"

fn test() {
    _ = io.discard()
}
"#,
    );

    // io.reader is an interface type, not a function
    rejects_e1002(
        r#"
package main

import "io"

fn test() {
    _ = io.reader()
}
"#,
    );

    // io.writer is an interface type, not a function
    rejects_e1002(
        r#"
package main

import "io"

fn test() {
    _ = io.writer()
}
"#,
    );

    // io.seek_start is an int constant, not a function
    rejects_e1002(
        r#"
package main

import "io"

fn test() {
    _ = io.seek_start()
}
"#,
    );
}

#[test]
fn test_io_variadic_spread_on_non_variadic_rejected() {
    // io.copy is non-variadic; spread `...` must be rejected
    rejects_e1002(
        r#"
package main

import "io"

fn test(args []io.reader) {
    _, _ = io.copy(args...)
}
"#,
    );

    // io.read_all is non-variadic; spread `...` must be rejected
    rejects_e1002(
        r#"
package main

import "io"

fn test(args []io.reader) {
    _, _ = io.read_all(args...)
}
"#,
    );
}

#[test]
fn test_io_positive_controls_accepted() {
    accepts(
        r#"
package main

import "io"

fn test(r io.reader, w io.writer, buf []byte) {
    _, _ = io.pipe()
    _, _ = io.read_all(r)
    _, _ = io.read_full(r, buf)
    _, _ = io.copy(w, r)
    _, _ = io.copy_n(w, r, 100)
    _, _ = io.write_string(w, "hello")
    _ = io.limit_reader(r, 1024)
    _ = io.tee_reader(r, w)
    _ = io.nop_closer(r)
    _ = io.multi_reader(r, r)
    _ = io.multi_writer(w, w)
}
"#,
    );
}
