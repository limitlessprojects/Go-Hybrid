//! Regression tests for #4118:
//! Curated foreign receiver methods (`bufio.reader`, `bufio.writer`, `bufio.scanner`,
//! `sync.pool`, `io.pipe_reader`, `io.pipe_writer`, `io.limited_reader`)
//! must enforce call arity constraints during `hyb check` (HYB-E1002, Row 13 BUILDABLE).
//!
//! Without arity checks on these foreign receiver methods, calls with too few or too many arguments
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
fn test_bufio_reader_arity_rejection() {
    // r.buffered takes 0 arguments
    rejects_e1002(
        r#"
package main

import "bufio"

fn test(r *bufio.reader) {
    r.buffered(1)
}
"#,
    );

    // r.discard takes 1 argument
    rejects_e1002(
        r#"
package main

import "bufio"

fn test(r *bufio.reader) {
    r.discard()
}
"#,
    );

    rejects_e1002(
        r#"
package main

import "bufio"

fn test(r *bufio.reader) {
    r.discard(1, 2)
}
"#,
    );

    // r.read_byte takes 0 arguments
    rejects_e1002(
        r#"
package main

import "bufio"

fn test(r *bufio.reader) {
    r.read_byte(1)
}
"#,
    );

    // r.size takes 0 arguments
    rejects_e1002(
        r#"
package main

import "bufio"

fn test(r *bufio.reader) {
    r.size(10)
}
"#,
    );
}

#[test]
fn test_bufio_writer_arity_rejection() {
    // w.flush takes 0 arguments
    rejects_e1002(
        r#"
package main

import "bufio"

fn test(w *bufio.writer) {
    w.flush(true)
}
"#,
    );

    // w.available takes 0 arguments
    rejects_e1002(
        r#"
package main

import "bufio"

fn test(w *bufio.writer) {
    w.available(1)
}
"#,
    );

    // w.write_string takes 1 argument
    rejects_e1002(
        r#"
package main

import "bufio"

fn test(w *bufio.writer) {
    w.write_string()
}
"#,
    );

    rejects_e1002(
        r#"
package main

import "bufio"

fn test(w *bufio.writer) {
    w.write_string("a", "b")
}
"#,
    );
}

#[test]
fn test_bufio_scanner_arity_rejection() {
    // s.scan takes 0 arguments
    rejects_e1002(
        r#"
package main

import "bufio"

fn test(s *bufio.scanner) {
    s.scan(1)
}
"#,
    );

    // s.text takes 0 arguments
    rejects_e1002(
        r#"
package main

import "bufio"

fn test(s *bufio.scanner) {
    s.text(42)
}
"#,
    );

    // s.split takes 1 argument
    rejects_e1002(
        r#"
package main

import "bufio"

fn test(s *bufio.scanner) {
    s.split()
}
"#,
    );

    // s.buffer takes 2 arguments
    rejects_e1002(
        r#"
package main

import "bufio"

fn test(s *bufio.scanner) {
    s.buffer()
}
"#,
    );
}

#[test]
fn test_sync_pool_arity_rejection() {
    // p.get takes 0 arguments
    rejects_e1002(
        r#"
package main

import "sync"

fn test(p *sync.pool) {
    p.get(1)
}
"#,
    );

    // p.put takes 1 argument
    rejects_e1002(
        r#"
package main

import "sync"

fn test(p *sync.pool) {
    p.put()
}
"#,
    );

    rejects_e1002(
        r#"
package main

import "sync"

fn test(p *sync.pool) {
    p.put(1, 2)
}
"#,
    );
}

#[test]
fn test_io_pipe_and_limited_reader_arity_rejection() {
    // pr.close takes 0 arguments
    rejects_e1002(
        r#"
package main

import "io"

fn test(pr *io.pipe_reader) {
    pr.close(1)
}
"#,
    );

    // pw.close takes 0 arguments
    rejects_e1002(
        r#"
package main

import "io"

fn test(pw *io.pipe_writer) {
    pw.close(1)
}
"#,
    );

    // lr.read takes 1 argument
    rejects_e1002(
        r#"
package main

import "io"

fn test(lr *io.limited_reader) {
    lr.read()
}
"#,
    );
}

#[test]
fn test_valid_foreign_receiver_method_calls_accepted() {
    accepts(
        r#"
package main

import (
    "bufio"
    "io"
    "sync"
)

fn test_valid(
    r *bufio.reader,
    w *bufio.writer,
    s *bufio.scanner,
    p *sync.pool,
    pr *io.pipe_reader,
    pw *io.pipe_writer,
) {
    r.buffered()
    r.discard(10)
    r.size()
    w.flush()
    w.available()
    w.write_string("ok")
    s.scan()
    s.text()
    p.get()
    p.put(100)
    pr.close()
    pw.close()
}
"#,
    );
}
