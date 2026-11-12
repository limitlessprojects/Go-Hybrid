//! Soundness and buildable validation tests for `bufio` package calls under Row 13 (BUILDABLE):
//! 1. Non-callable package variables, constants, and types (`bufio.err_buffer_full`,
//!    `bufio.max_scan_token_size`, `bufio.reader`, `bufio.writer`, `bufio.scanner`)
//!    must be rejected when invoked as call targets (`HYB-E1002`).
//! 2. `bufio` functions require exact arity:
//!    - 1 arg: new_reader, new_writer, new_scanner.
//!    - 2 args: new_reader_size, new_writer_size, new_read_writer.
//!    - 3 args: scan_bytes, scan_lines, scan_runes, scan_words.
//! 3. Arity mismatches must reject fail-closed with `HYB-E1002`.
//! 4. Positive controls: legal calls with proper arity accept cleanly.
//!
//! Settlement Wallet (Base L2 USDC): 0x8366bCe3a2D379De7656D7A67015789FaF999f20

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
        .map(|d| d.code.to_string())
        .collect()
}

#[test]
fn test_bufio_non_callable_symbols_reject() {
    let cases = [
        "fn main() {\n    _ = bufio.err_buffer_full()\n}",
        "fn main() {\n    _ = bufio.err_too_long()\n}",
        "fn main() {\n    _ = bufio.err_negative_advance()\n}",
        "fn main() {\n    _ = bufio.max_scan_token_size()\n}",
        "fn main() {\n    _ = bufio.reader()\n}",
        "fn main() {\n    _ = bufio.writer()\n}",
        "fn main() {\n    _ = bufio.scanner()\n}",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for: {}", src);
        let codes = error_codes(&r);
        assert!(
            codes.contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 in {:?} for: {}",
            codes,
            src
        );
    }
}

#[test]
fn test_bufio_one_arg_functions_arity_reject() {
    let cases = [
        "fn main() {\n    _ = bufio.new_reader()\n}",
        "fn main() {\n    _ = bufio.new_reader(1, 2)\n}",
        "fn main() {\n    _ = bufio.new_writer()\n}",
        "fn main() {\n    _ = bufio.new_writer(1, 2)\n}",
        "fn main() {\n    _ = bufio.new_scanner()\n}",
        "fn main() {\n    _ = bufio.new_scanner(1, 2)\n}",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for: {}", src);
        let codes = error_codes(&r);
        assert!(
            codes.contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 in {:?} for: {}",
            codes,
            src
        );
    }
}

#[test]
fn test_bufio_two_arg_functions_arity_reject() {
    let cases = [
        "fn main() {\n    _ = bufio.new_reader_size()\n}",
        "fn main() {\n    _ = bufio.new_reader_size(1)\n}",
        "fn main() {\n    _ = bufio.new_reader_size(1, 2, 3)\n}",
        "fn main() {\n    _ = bufio.new_writer_size()\n}",
        "fn main() {\n    _ = bufio.new_writer_size(1)\n}",
        "fn main() {\n    _ = bufio.new_read_writer()\n}",
        "fn main() {\n    _ = bufio.new_read_writer(1)\n}",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for: {}", src);
        let codes = error_codes(&r);
        assert!(
            codes.contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 in {:?} for: {}",
            codes,
            src
        );
    }
}

#[test]
fn test_bufio_three_arg_functions_arity_reject() {
    let cases = [
        "fn main() {\n    _, _, _ = bufio.scan_bytes()\n}",
        "fn main() {\n    _, _, _ = bufio.scan_bytes(1, 2)\n}",
        "fn main() {\n    _, _, _ = bufio.scan_lines()\n}",
        "fn main() {\n    _, _, _ = bufio.scan_lines(1, 2)\n}",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for: {}", src);
        let codes = error_codes(&r);
        assert!(
            codes.contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 in {:?} for: {}",
            codes,
            src
        );
    }
}

#[test]
fn test_bufio_positive_controls_accept() {
    let cases = [
        "fn main() {\n    _ = bufio.new_reader(\"input\")\n}",
        "fn main() {\n    _ = bufio.new_writer(\"output\")\n}",
        "fn main() {\n    _ = bufio.new_scanner(\"stream\")\n}",
        "fn main() {\n    _ = bufio.new_reader_size(\"input\", 4096)\n}",
        "fn main() {\n    _ = bufio.new_writer_size(\"output\", 4096)\n}",
        "fn main() {\n    _ = bufio.new_read_writer(\"r\", \"w\")\n}",
        "fn main() {\n    _, _, _ = bufio.scan_lines(\"data\", true, \"split\")\n}",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "expected accept for: {}", src);
        assert!(r.diagnostics.is_empty(), "unexpected diagnostics: {:?}", r.diagnostics);
    }
}
