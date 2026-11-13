//! bufio_call_buildable_soundness.rs — regression tests for bufio member call arity and non-callable symbols
//! (HYB-E1002, Row 13 BUILDABLE soundness).
//!
//! Go standard library bufio contract:
//! - `bufio.NewReader(rd)` takes exactly 1 argument.
//! - `bufio.NewReaderSize(rd, size)` takes exactly 2 arguments.
//! - `bufio.NewWriter(w)` takes exactly 1 argument.
//! - `bufio.NewWriterSize(w, size)` takes exactly 2 arguments.
//! - `bufio.NewReadWriter(r, w)` takes exactly 2 arguments.
//! - `bufio.NewScanner(r)` takes exactly 1 argument.
//! - `bufio.ScanBytes(data, atEOF)` takes exactly 2 arguments.
//! - `bufio.ScanLines(data, atEOF)` takes exactly 2 arguments.
//! - `bufio.ScanRunes(data, atEOF)` takes exactly 2 arguments.
//! - `bufio.ScanWords(data, atEOF)` takes exactly 2 arguments.
//! - Non-callable variables/constants/types (`bufio.ErrBufferFull`, `bufio.MaxScanTokenSize`, `bufio.Reader`, etc.)
//!   must be rejected with HYB-E1002 when called as functions.

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
fn test_bufio_non_callable_members_rejected() {
    rejects_e1002(
        r#"
package main
import "bufio"
fn test() {
    _ = bufio.err_buffer_full()
}
"#,
    );

    rejects_e1002(
        r#"
package main
import "bufio"
fn test() {
    _ = bufio.max_scan_token_size()
}
"#,
    );

    rejects_e1002(
        r#"
package main
import "bufio"
fn test() {
    _ = bufio.reader()
}
"#,
    );

    rejects_e1002(
        r#"
package main
import "bufio"
fn test() {
    _ = bufio.writer()
}
"#,
    );

    rejects_e1002(
        r#"
package main
import "bufio"
fn test() {
    _ = bufio.scanner()
}
"#,
    );
}

#[test]
fn test_bufio_call_arity_rejected() {
    // new_reader takes 1 arg, called with 0
    rejects_e1002(
        r#"
package main
import "bufio"
fn test() {
    _ = bufio.new_reader()
}
"#,
    );

    // new_reader takes 1 arg, called with 2
    rejects_e1002(
        r#"
package main
import "bufio"
import "os"
fn test() {
    _ = bufio.new_reader(os.stdin, 1024)
}
"#,
    );

    // new_writer takes 1 arg, called with 0
    rejects_e1002(
        r#"
package main
import "bufio"
fn test() {
    _ = bufio.new_writer()
}
"#,
    );

    // new_reader_size takes 2 args, called with 1
    rejects_e1002(
        r#"
package main
import "bufio"
import "os"
fn test() {
    _ = bufio.new_reader_size(os.stdin)
}
"#,
    );

    // new_scanner takes 1 arg, called with 0
    rejects_e1002(
        r#"
package main
import "bufio"
fn test() {
    _ = bufio.new_scanner()
}
"#,
    );
}

#[test]
fn test_bufio_positive_controls_accepted() {
    accepts(
        r#"
package main
import "bufio"
import "os"
fn test() {
    r := bufio.new_reader(os.stdin)
    _ = r
    w := bufio.new_writer(os.stdout)
    _ = w
    s := bufio.new_scanner(os.stdin)
    _ = s
    rs := bufio.new_reader_size(os.stdin, 4096)
    _ = rs
    ws := bufio.new_writer_size(os.stdout, 4096)
    _ = ws
}
"#,
    );
}
