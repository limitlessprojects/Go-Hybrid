//! gzip_call_buildable_soundness.rs — regression tests for compress/gzip member call arity and non-callable symbols
//! (HYB-E1002, Row 13 BUILDABLE soundness).
//!
//! Go standard library contract:
//! - `gzip.NewReader(r)` takes exactly 1 argument `(r)`.
//! - `gzip.NewWriter(w)` takes exactly 1 argument `(w)`.
//! - `gzip.NewWriterLevel(w, level)` takes exactly 2 arguments `(w, level)`.
//! - Non-callable compression level constants (`gzip.NoCompression`, `gzip.BestSpeed`, `gzip.BestCompression`,
//!   `gzip.DefaultCompression`, `gzip.HuffmanOnly`), error variables (`gzip.ErrChecksum`, `gzip.ErrHeader`),
//!   and struct types must be rejected when invoked as functions.

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
fn test_gzip_one_arg_functions_arity_reject() {
    // gzip.new_reader requires exactly 1 argument
    rejects_e1002(
        r#"package main
import "compress/gzip"

fn main() {
    _ = gzip.new_reader()
}
"#,
    );
    rejects_e1002(
        r#"package main
import "compress/gzip"

fn main() {
    _ = gzip.new_reader(1, 2)
}
"#,
    );

    // gzip.new_writer requires exactly 1 argument
    rejects_e1002(
        r#"package main
import "compress/gzip"

fn main() {
    _ = gzip.new_writer()
}
"#,
    );
    rejects_e1002(
        r#"package main
import "compress/gzip"

fn main() {
    _ = gzip.new_writer(1, 2)
}
"#,
    );
}

#[test]
fn test_gzip_two_arg_functions_arity_reject() {
    // gzip.new_writer_level requires exactly 2 arguments
    rejects_e1002(
        r#"package main
import "compress/gzip"

fn main() {
    _ = gzip.new_writer_level()
}
"#,
    );
    rejects_e1002(
        r#"package main
import "compress/gzip"

fn main() {
    _ = gzip.new_writer_level(1)
}
"#,
    );
    rejects_e1002(
        r#"package main
import "compress/gzip"

fn main() {
    _ = gzip.new_writer_level(1, 2, 3)
}
"#,
    );
}

#[test]
fn test_gzip_non_callable_symbols_reject() {
    rejects_e1002(
        r#"package main
import "compress/gzip"

fn main() {
    _ = gzip.no_compression()
}
"#,
    );
    rejects_e1002(
        r#"package main
import "compress/gzip"

fn main() {
    _ = gzip.best_speed()
}
"#,
    );
    rejects_e1002(
        r#"package main
import "compress/gzip"

fn main() {
    _ = gzip.best_compression()
}
"#,
    );
    rejects_e1002(
        r#"package main
import "compress/gzip"

fn main() {
    _ = gzip.default_compression()
}
"#,
    );
    rejects_e1002(
        r#"package main
import "compress/gzip"

fn main() {
    _ = gzip.huffman_only()
}
"#,
    );
    rejects_e1002(
        r#"package main
import "compress/gzip"

fn main() {
    _ = gzip.err_checksum()
}
"#,
    );
    rejects_e1002(
        r#"package main
import "compress/gzip"

fn main() {
    _ = gzip.err_header()
}
"#,
    );
}

#[test]
fn test_gzip_positive_controls_accept() {
    // Shadowed package maintains user semantics
    accepts(
        r#"
struct my_gzip {
    val int
}

fn (g my_gzip) new_reader() int {
    return g.val
}

fn main() {
    gzip := my_gzip{val: 42}
    _ = gzip.new_reader()
}
"#,
    );
}
