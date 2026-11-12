//! Regression tests for foreign curated package `bufio` call-arity and symbol validation.
//! Enforces GUARANTEES.md Row 13 (BUILDABLE) and HYB-E1002 (Fixes #4216).

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
        "expected Accept with 0 errors for:\n{}\ngot: {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept for:\n{}", src);
}

#[test]
fn test_bufio_new_reader_arity() {
    // 0 args -> reject
    rejects_e1002("import \"bufio\"\nfn main() { _ = bufio.new_reader() }");
    // 2 args -> reject
    rejects_e1002("import \"bufio\"\nimport \"os\"\nfn main() { _ = bufio.new_reader(os.stdin, 123) }");
    // spread -> reject
    rejects_e1002("import \"bufio\"\nfn main() {\n    xs := []any{1}\n    _ = bufio.new_reader(xs...)\n}");
}

#[test]
fn test_bufio_new_writer_arity() {
    // 0 args -> reject
    rejects_e1002("import \"bufio\"\nfn main() { _ = bufio.new_writer() }");
    // 2 args -> reject
    rejects_e1002("import \"bufio\"\nimport \"os\"\nfn main() { _ = bufio.new_writer(os.stdout, 123) }");
}

#[test]
fn test_bufio_new_scanner_arity() {
    // 0 args -> reject
    rejects_e1002("import \"bufio\"\nfn main() { _ = bufio.new_scanner() }");
    // 2 args -> reject
    rejects_e1002("import \"bufio\"\nimport \"os\"\nfn main() { _ = bufio.new_scanner(os.stdin, 123) }");
}

#[test]
fn test_bufio_two_arg_functions_arity() {
    // new_reader_size requires 2 args
    rejects_e1002("import \"bufio\"\nimport \"os\"\nfn main() { _ = bufio.new_reader_size(os.stdin) }");
    rejects_e1002("import \"bufio\"\nimport \"os\"\nfn main() { _ = bufio.new_reader_size(os.stdin, 1024, 2048) }");

    // new_writer_size requires 2 args
    rejects_e1002("import \"bufio\"\nimport \"os\"\nfn main() { _ = bufio.new_writer_size(os.stdout) }");

    // new_read_writer requires 2 args
    rejects_e1002("import \"bufio\"\nfn main() { _ = bufio.new_read_writer() }");
}

#[test]
fn test_bufio_non_callable_symbols_reject() {
    // Package types
    rejects_e1002("import \"bufio\"\nfn main() { _ = bufio.reader() }");
    rejects_e1002("import \"bufio\"\nfn main() { _ = bufio.writer() }");
    rejects_e1002("import \"bufio\"\nfn main() { _ = bufio.scanner() }");
    rejects_e1002("import \"bufio\"\nfn main() { _ = bufio.read_writer() }");
    rejects_e1002("import \"bufio\"\nfn main() { _ = bufio.split_func() }");

    // Package constants
    rejects_e1002("import \"bufio\"\nfn main() { _ = bufio.max_scan_token_size() }");

    // Package error variables
    rejects_e1002("import \"bufio\"\nfn main() { _ = bufio.err_buffer_full() }");
    rejects_e1002("import \"bufio\"\nfn main() { _ = bufio.err_too_long() }");
    rejects_e1002("import \"bufio\"\nfn main() { _ = bufio.err_advance_too_far() }");
    rejects_e1002("import \"bufio\"\nfn main() { _ = bufio.err_bad_read_count() }");
    rejects_e1002("import \"bufio\"\nfn main() { _ = bufio.err_final_token() }");
    rejects_e1002("import \"bufio\"\nfn main() { _ = bufio.err_invalid_unread_byte() }");
    rejects_e1002("import \"bufio\"\nfn main() { _ = bufio.err_invalid_unread_rune() }");
    rejects_e1002("import \"bufio\"\nfn main() { _ = bufio.err_negative_advance() }");
    rejects_e1002("import \"bufio\"\nfn main() { _ = bufio.err_negative_count() }");
}

#[test]
fn test_bufio_positive_controls() {
    let src = r#"
import "bufio"
import "os"

fn test_valid() {
    r := bufio.new_reader(os.stdin)
    _ = r
    w := bufio.new_writer(os.stdout)
    _ = w
    rs := bufio.new_reader_size(os.stdin, 4096)
    _ = rs
    ws := bufio.new_writer_size(os.stdout, 4096)
    _ = ws
    s := bufio.new_scanner(os.stdin)
    _ = s
}
"#;
    accepts(src);
}

#[test]
fn test_bufio_shadowing_preserves_semantics() {
    let src = r#"
fn test_shadowing() {
    var bufio = 42
    _ = bufio
    bufio2 := "hello"
    _ = bufio2
}
"#;
    accepts(src);
}
