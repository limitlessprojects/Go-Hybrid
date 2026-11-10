//! Tests for the curated foreign method surface of `bytes.buffer`.
//! Guarantees that calling phantom / unsupported methods on the curated
//! `bytes.buffer` foreign surface rejects fail-closed with HYB-E1002, and
//! every declared method name is accepted outright (Row 13 BUILDABLE).
//!
//! Surface under test (declared in hy-check/src/foreign.rs, BYTES_BUFFER):
//! write, write_byte, write_rune, write_string, write_to, read, read_byte,
//! read_bytes, read_from, read_rune, read_string, next, grow, reset, string,
//! bytes, len, cap, truncate, unread_byte, unread_rune.

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn phantom_buffer_methods_are_rejected() {
    let bad = check(
        "import \"bytes\"\n\
         fn inspect(b *bytes.buffer) {\n\
             b.totally_fake()\n\
         }\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject);
    assert!(codes(&bad).iter().any(|c| c == "HYB-E1002"));
    let diag = bad.diagnostics.iter().find(|d| d.code == "HYB-E1002").unwrap();
    assert!(diag.message.contains("bytes.buffer"), "{}", diag.message);
}

#[test]
fn every_declared_buffer_method_is_accepted() {
    // Positive control: each declared method name must be accepted outright.
    for m in [
        "write", "write_byte", "write_rune", "write_string", "write_to",
        "read", "read_byte", "read_bytes", "read_from", "read_rune",
        "read_string", "next", "grow", "reset", "string", "bytes", "len",
        "cap", "truncate", "unread_byte", "unread_rune",
    ] {
        let ok = check(&format!(
            "import \"bytes\"\nfn inspect(b *bytes.buffer) {{\n    b.{m}()\n}}\n"
        ));
        assert_eq!(
            ok.verdict,
            Verdict::Accept,
            "declared method {m} must be accepted; got {:?}",
            codes(&ok)
        );
    }
}
