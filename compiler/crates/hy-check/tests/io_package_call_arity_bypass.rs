//! Regression tests for Issue #4149 / Supplement to #4204:
//! Curated foreign package function calls (`io.*`) must enforce call arity
//! constraints and non-callable member checks during `hyb check` (HYB-E1002, Row 13 BUILDABLE).
//!
//! Extends coverage to:
//! - Complete 37 non-callable error variables, variables, constants, and interface/struct types
//! - `io.nop_closer(r)` arity checking
//! - Rejection of `...` variadic spread syntax on non-variadic `io` functions

use hy_check::{check_file, CheckResult, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
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

    // io.nop_closer takes 1 argument, called with 0
    rejects_e1002(
        r#"
package main

import "io"

fn test() {
    _ = io.nop_closer()
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

    // io.tee_reader takes 2 arguments, called with 1
    rejects_e1002(
        r#"
package main

import "io"

fn test(r io.reader) {
    _ = io.tee_reader(r)
}
"#,
    );

    // io.write_string takes 2 arguments, called with 1
    rejects_e1002(
        r#"
package main

import "io"

fn test(w io.writer) {
    _, _ = io.write_string(w)
}
"#,
    );

    // io.copy_buffer takes 3 arguments, called with 2
    rejects_e1002(
        r#"
package main

import "io"

fn test(w io.writer, r io.reader) {
    _, _ = io.copy_buffer(w, r)
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

    // io.read_at_least takes 3 arguments, called with 2
    rejects_e1002(
        r#"
package main

import "io"

fn test(r io.reader, buf []byte) {
    _, _ = io.read_at_least(r, buf)
}
"#,
    );

    // io.new_section_reader takes 3 arguments (r, off, n), called with 2
    rejects_e1002(
        r#"
package main

import "io"

fn test(r io.reader_at) {
    _ = io.new_section_reader(r, 0)
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

    // io.nop_closer takes 1 argument, called with 2
    rejects_e1002(
        r#"
package main

import "io"

fn test(r io.reader) {
    _ = io.nop_closer(r, "extra")
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

    // io.limit_reader takes 2 arguments, called with 3
    rejects_e1002(
        r#"
package main

import "io"

fn test(r io.reader) {
    _ = io.limit_reader(r, 10, "extra")
}
"#,
    );

    // io.read_full takes 2 arguments, called with 3
    rejects_e1002(
        r#"
package main

import "io"

fn test(r io.reader, buf []byte) {
    _, _ = io.read_full(r, buf, "extra")
}
"#,
    );

    // io.tee_reader takes 2 arguments, called with 3
    rejects_e1002(
        r#"
package main

import "io"

fn test(r io.reader, w io.writer) {
    _ = io.tee_reader(r, w, "extra")
}
"#,
    );

    // io.write_string takes 2 arguments, called with 3
    rejects_e1002(
        r#"
package main

import "io"

fn test(w io.writer) {
    _, _ = io.write_string(w, "hello", "extra")
}
"#,
    );

    // io.copy_buffer takes 3 arguments, called with 4
    rejects_e1002(
        r#"
package main

import "io"

fn test(w io.writer, r io.reader, buf []byte) {
    _, _ = io.copy_buffer(w, r, buf, "extra")
}
"#,
    );

    // io.copy_n takes 3 arguments, called with 4
    rejects_e1002(
        r#"
package main

import "io"

fn test(w io.writer, r io.reader) {
    _, _ = io.copy_n(w, r, 100, "extra")
}
"#,
    );

    // io.read_at_least takes 3 arguments, called with 4
    rejects_e1002(
        r#"
package main

import "io"

fn test(r io.reader, buf []byte) {
    _, _ = io.read_at_least(r, buf, 10, "extra")
}
"#,
    );

    // io.new_section_reader takes 3 arguments (r, off, n), called with 4
    rejects_e1002(
        r#"
package main

import "io"

fn test(r io.reader_at) {
    _ = io.new_section_reader(r, 0, 100, "extra")
}
"#,
    );
}

#[test]
fn test_io_non_callable_members_rejected() {
    // Package error variables
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.eof() }\n");
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.err_closed_pipe() }\n");
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.err_no_progress() }\n");
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.err_short_buffer() }\n");
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.err_short_write() }\n");
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.err_unexpected_eof() }\n");

    // Package variables
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.discard() }\n");

    // Constants
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.seek_start() }\n");
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.seek_current() }\n");
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.seek_end() }\n");

    // Interface and struct types
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.reader() }\n");
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.writer() }\n");
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.closer() }\n");
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.read_writer() }\n");
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.read_closer() }\n");
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.write_closer() }\n");
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.read_write_closer() }\n");
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.read_seeker() }\n");
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.read_seek_closer() }\n");
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.write_seeker() }\n");
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.read_write_seeker() }\n");
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.reader_from() }\n");
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.writer_to() }\n");
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.reader_at() }\n");
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.writer_at() }\n");
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.byte_reader() }\n");
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.byte_scanner() }\n");
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.byte_writer() }\n");
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.rune_reader() }\n");
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.rune_scanner() }\n");
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.string_writer() }\n");
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.limited_reader() }\n");
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.section_reader() }\n");
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.offset_writer() }\n");
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.pipe_reader() }\n");
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.pipe_writer() }\n");
    rejects_e1002("package main\nimport \"io\"\nfn test() { _ = io.seeker() }\n");
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

    // io.nop_closer is non-variadic; spread `...` must be rejected
    rejects_e1002(
        r#"
package main

import "io"

fn test(args []io.reader) {
    _ = io.nop_closer(args...)
}
"#,
    );

    // io.new_section_reader is non-variadic; spread `...` must be rejected
    rejects_e1002(
        r#"
package main

import "io"

fn test(args []io.reader_at) {
    _ = io.new_section_reader(args...)
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

fn test(r io.reader, w io.writer, ra io.reader_at, buf []byte) {
    _, _ = io.pipe()
    _, _ = io.read_all(r)
    _, _ = io.read_full(r, buf)
    _, _ = io.copy(w, r)
    _, _ = io.copy_buffer(w, r, buf)
    _, _ = io.copy_n(w, r, 100)
    _, _ = io.read_at_least(r, buf, 10)
    _, _ = io.write_string(w, "hello")
    _ = io.limit_reader(r, 1024)
    _ = io.tee_reader(r, w)
    _ = io.nop_closer(r)
    _ = io.new_section_reader(ra, 0, 100)
    _ = io.multi_reader(r, r)
    _ = io.multi_writer(w, w)
}
"#,
    );
}
