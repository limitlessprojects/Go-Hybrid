//! Soundness and buildable validation tests for `encoding/hex` (`hex`) calls under Row 13 (BUILDABLE):
//! 1. Non-callable package variables and types (`hex.err_length`, `hex.invalid_byte_error`)
//!    must be rejected when invoked as call targets (`HYB-E1002`).
//! 2. `hex` functions require exact arity:
//!    - 1 arg: encode, encode_to_string, decode_string, encoded_len, decoded_len, dump, dumper, new_encoder, new_decoder.
//!    - 2 args: decode.
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
fn test_hex_non_callable_symbols_reject() {
    let cases = [
        "fn main() {\n    _ = hex.err_length()\n}",
        "fn main() {\n    _ = hex.invalid_byte_error()\n}",
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
fn test_hex_one_arg_functions_arity_reject() {
    let cases = [
        "fn main() {\n    _ = hex.encode()\n}",
        "fn main() {\n    _ = hex.encode(\"a\", \"b\")\n}",
        "fn main() {\n    _ = hex.encode_to_string()\n}",
        "fn main() {\n    _ = hex.encode_to_string(\"a\", \"b\")\n}",
        "fn main() {\n    _, _ = hex.decode_string()\n}",
        "fn main() {\n    _, _ = hex.decode_string(\"a\", \"b\")\n}",
        "fn main() {\n    _ = hex.encoded_len()\n}",
        "fn main() {\n    _ = hex.encoded_len(1, 2)\n}",
        "fn main() {\n    _ = hex.decoded_len()\n}",
        "fn main() {\n    _ = hex.decoded_len(1, 2)\n}",
        "fn main() {\n    _ = hex.dump()\n}",
        "fn main() {\n    _ = hex.dump(\"a\", \"b\")\n}",
        "fn main() {\n    _ = hex.dumper()\n}",
        "fn main() {\n    _ = hex.new_encoder()\n}",
        "fn main() {\n    _ = hex.new_decoder()\n}",
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
fn test_hex_two_arg_functions_arity_reject() {
    let cases = [
        "fn main() {\n    _, _ = hex.decode()\n}",
        "fn main() {\n    _, _ = hex.decode(\"a\")\n}",
        "fn main() {\n    _, _ = hex.decode(\"a\", \"b\", \"c\")\n}",
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
fn test_hex_positive_controls_accept() {
    let cases = [
        "fn main() {\n    _ = hex.encode(\"hello\")\n}",
        "fn main() {\n    _ = hex.encode_to_string(\"hello\")\n}",
        "fn main() {\n    _, _ = hex.decode_string(\"68656c6c6f\")\n}",
        "fn main() {\n    _ = hex.encoded_len(10)\n}",
        "fn main() {\n    _ = hex.decoded_len(20)\n}",
        "fn main() {\n    _ = hex.dump(\"data\")\n}",
        "fn main() {\n    _, _ = hex.decode(\"dst\", \"src\")\n}",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "expected accept for: {}", src);
        assert!(r.diagnostics.is_empty(), "unexpected diagnostics: {:?}", r.diagnostics);
    }
}
