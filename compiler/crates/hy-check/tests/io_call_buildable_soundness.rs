//! io_call_buildable_soundness.rs — regression tests for io member call arity and non-callable symbols
//! (HYB-E1002, Row 13 BUILDABLE soundness).
//!
//! Go standard library contract:
//! - `io.Pipe()` takes exactly 0 arguments.
//! - `io.ReadAll(r)` takes exactly 1 argument.
//! - `io.Copy(dst, src)` takes exactly 2 arguments.
//! - `io.ReadFull(r, buf)` takes exactly 2 arguments.
//! - `io.WriteString(w, s)` takes exactly 2 arguments.
//! - `io.LimitReader(r, n)` takes exactly 2 arguments.
//! - `io.TeeReader(r, w)` takes exactly 2 arguments.
//! - `io.CopyN(dst, src, n)` takes exactly 3 arguments.
//! - `io.CopyBuffer(dst, src, buf)` takes exactly 3 arguments.
//! - `io.ReadAtLeast(r, buf, min)` takes exactly 3 arguments.
//! - `io.NewSectionReader(r, off, n)` takes exactly 3 arguments.
//! - `io.MultiReader(readers...)` and `io.MultiWriter(writers...)` require >= 1 argument.
//! - Non-callable variables/constants/types (`io.EOF`, `io.ErrUnexpectedEOF`, `io.Discard`, `io.Reader`, etc.)
//!   must be rejected when called as functions.

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
fn test_io_zero_arg_functions_arity_reject() {
    // io.pipe takes 0 arguments
    rejects_e1002(
        r#"package main
import "io"

fn main() {
    _ = io.pipe(1)
}
"#,
    );
}

#[test]
fn test_io_one_arg_functions_arity_reject() {
    // io.read_all requires exactly 1 argument
    rejects_e1002(
        r#"package main
import "io"

fn main() {
    _ = io.read_all()
}
"#,
    );
    rejects_e1002(
        r#"package main
import "io"

fn main() {
    _ = io.read_all(1, 2)
}
"#,
    );
}

#[test]
fn test_io_two_arg_functions_arity_reject() {
    // io.copy requires 2 arguments
    rejects_e1002(
        r#"package main
import "io"

fn main() {
    _ = io.copy()
}
"#,
    );
    rejects_e1002(
        r#"package main
import "io"

fn main() {
    _ = io.copy(1)
}
"#,
    );
    rejects_e1002(
        r#"package main
import "io"

fn main() {
    _ = io.copy(1, 2, 3)
}
"#,
    );

    // io.read_full requires 2 arguments
    rejects_e1002(
        r#"package main
import "io"

fn main() {
    _ = io.read_full()
}
"#,
    );
    rejects_e1002(
        r#"package main
import "io"

fn main() {
    _ = io.read_full(1)
}
"#,
    );

    // io.write_string requires 2 arguments
    rejects_e1002(
        r#"package main
import "io"

fn main() {
    _ = io.write_string()
}
"#,
    );
    rejects_e1002(
        r#"package main
import "io"

fn main() {
    _ = io.write_string("hello")
}
"#,
    );
}

#[test]
fn test_io_three_arg_functions_arity_reject() {
    // io.copy_n requires 3 arguments
    rejects_e1002(
        r#"package main
import "io"

fn main() {
    _ = io.copy_n()
}
"#,
    );
    rejects_e1002(
        r#"package main
import "io"

fn main() {
    _ = io.copy_n(1, 2)
}
"#,
    );

    // io.copy_buffer requires 3 arguments
    rejects_e1002(
        r#"package main
import "io"

fn main() {
    _ = io.copy_buffer(1, 2)
}
"#,
    );

    // io.read_at_least requires 3 arguments
    rejects_e1002(
        r#"package main
import "io"

fn main() {
    _ = io.read_at_least(1, 2)
}
"#,
    );

    // io.new_section_reader requires 3 arguments
    rejects_e1002(
        r#"package main
import "io"

fn main() {
    _ = io.new_section_reader(1, 2)
}
"#,
    );
}

#[test]
fn test_io_variadic_functions_arity_reject() {
    // io.multi_reader requires at least 1 argument
    rejects_e1002(
        r#"package main
import "io"

fn main() {
    _ = io.multi_reader()
}
"#,
    );

    // io.multi_writer requires at least 1 argument
    rejects_e1002(
        r#"package main
import "io"

fn main() {
    _ = io.multi_writer()
}
"#,
    );
}

#[test]
fn test_io_non_callable_symbols_reject() {
    rejects_e1002(
        r#"package main
import "io"

fn main() {
    _ = io.eof()
}
"#,
    );
    rejects_e1002(
        r#"package main
import "io"

fn main() {
    _ = io.err_unexpected_eof()
}
"#,
    );
    rejects_e1002(
        r#"package main
import "io"

fn main() {
    _ = io.err_no_progress()
}
"#,
    );
    rejects_e1002(
        r#"package main
import "io"

fn main() {
    _ = io.discard()
}
"#,
    );
    rejects_e1002(
        r#"package main
import "io"

fn main() {
    _ = io.reader()
}
"#,
    );
}

#[test]
fn test_io_positive_controls_accept() {
    accepts(
        r#"package main
import "io"

fn main() {
    var p = io.eof
    _ = p
}
"#,
    );
    accepts(
        r#"package main
import "io"

fn main() {
    var d = io.discard
    _ = d
}
"#,
    );
}
