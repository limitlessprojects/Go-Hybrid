//! Soundness tests for `encoding/base64` call arity and non-callable symbol rejection (HYB-E1002, Row 13).
//! Under GUARANTEES.md Row 13 (BUILDABLE), any source program accepted by hy-check with verdict Accept
//! must compile cleanly under standard Go's toolchain (go build).
//!
//! Enforces:
//! 1. Package variables, constants, and types are rejected as call targets with HYB-E1002.
//! 2. Constructor functions (`new_encoding`, `new_decoder`, `new_encoder`) validate exact call arity.
//! 3. Valid invocations and symbol references accept cleanly with 0 diagnostics.

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn test_base64_non_callable_variables_rejected() {
    let bad = check(
        "import \"encoding/base64\"\n\
         fn main() {\n\
             _ = base64.std_encoding()\n\
             _ = base64.url_encoding()\n\
             _ = base64.raw_std_encoding()\n\
             _ = base64.raw_url_encoding()\n\
         }\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject, "expected reject, got {:?}", bad.diagnostics);
    let all_codes = codes(&bad);
    assert_eq!(all_codes.iter().filter(|c| *c == "HYB-E1002").count(), 4, "expected 4 HYB-E1002 errors, got {:?}", all_codes);
    for diag in &bad.diagnostics {
        assert!(diag.message.contains("not a function"), "diag message: {}", diag.message);
    }
}

#[test]
fn test_base64_non_callable_constants_and_types_rejected() {
    let bad = check(
        "import \"encoding/base64\"\n\
         fn main() {\n\
             _ = base64.std_padding()\n\
             _ = base64.no_padding()\n\
             _ = base64.encoding()\n\
             _ = base64.corrupt_input_error()\n\
         }\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject, "expected reject, got {:?}", bad.diagnostics);
    let all_codes = codes(&bad);
    assert_eq!(all_codes.iter().filter(|c| *c == "HYB-E1002").count(), 4, "expected 4 HYB-E1002 errors, got {:?}", all_codes);
}

#[test]
fn test_base64_new_encoding_arity_rejected() {
    let bad_zero = check(
        "import \"encoding/base64\"\n\
         fn main() {\n\
             _ = base64.new_encoding()\n\
         }\n",
    );
    assert_eq!(bad_zero.verdict, Verdict::Reject, "expected reject, got {:?}", bad_zero.diagnostics);
    assert!(bad_zero.diagnostics.iter().any(|d| d.code == "HYB-E1002" && d.message.contains("base64.new_encoding")));

    let bad_two = check(
        "import \"encoding/base64\"\n\
         fn main() {\n\
             _ = base64.new_encoding(\"a\", \"b\")\n\
         }\n",
    );
    assert_eq!(bad_two.verdict, Verdict::Reject, "expected reject, got {:?}", bad_two.diagnostics);
    assert!(bad_two.diagnostics.iter().any(|d| d.code == "HYB-E1002" && d.message.contains("base64.new_encoding")));
}

#[test]
fn test_base64_new_decoder_arity_rejected() {
    let bad_one = check(
        "import \"encoding/base64\"\n\
         fn main() {\n\
             _ = base64.new_decoder(base64.std_encoding)\n\
         }\n",
    );
    assert_eq!(bad_one.verdict, Verdict::Reject, "expected reject, got {:?}", bad_one.diagnostics);
    assert!(bad_one.diagnostics.iter().any(|d| d.code == "HYB-E1002" && d.message.contains("base64.new_decoder")));

    let bad_three = check(
        "import \"encoding/base64\"\n\
         fn main() {\n\
             _ = base64.new_decoder(base64.std_encoding, 1, 2)\n\
         }\n",
    );
    assert_eq!(bad_three.verdict, Verdict::Reject, "expected reject, got {:?}", bad_three.diagnostics);
    assert!(bad_three.diagnostics.iter().any(|d| d.code == "HYB-E1002" && d.message.contains("base64.new_decoder")));
}

#[test]
fn test_base64_new_encoder_arity_rejected() {
    let bad_zero = check(
        "import \"encoding/base64\"\n\
         fn main() {\n\
             _ = base64.new_encoder()\n\
         }\n",
    );
    assert_eq!(bad_zero.verdict, Verdict::Reject, "expected reject, got {:?}", bad_zero.diagnostics);
    assert!(bad_zero.diagnostics.iter().any(|d| d.code == "HYB-E1002" && d.message.contains("base64.new_encoder")));
}

#[test]
fn test_base64_valid_constructors_and_references_accept() {
    let good = check(
        "import \"encoding/base64\"\n\
         fn main() {\n\
             enc := base64.new_encoding(\"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/\")\n\
             _ = enc\n\
             _ = base64.std_encoding\n\
             _ = base64.url_encoding\n\
             _ = base64.raw_std_encoding\n\
             _ = base64.raw_url_encoding\n\
             _ = base64.std_padding\n\
             _ = base64.no_padding\n\
         }\n",
    );
    assert_eq!(good.verdict, Verdict::Accept, "expected accept, got {:?}", good.diagnostics);
    assert!(good.diagnostics.is_empty(), "expected 0 diagnostics, got {:?}", good.diagnostics);
}
