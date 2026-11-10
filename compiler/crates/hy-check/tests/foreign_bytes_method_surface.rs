//! DRAFT (FOM2/Fo, 2026-11-10 05:05 IST — NOT SUBMITTED; needs QC green on a
//! C-toolchain env or fork CI before any PR). Modeled on
//! tests/foreign_io_method_surfaces.rs: guarantees that calling phantom /
//! unsupported methods on the curated `bytes.buffer` foreign surface rejects
//! fail-closed with HYB-E1002 (Row 13 BUILDABLE).
//!
//! Surface under test (declared in hy-check/src/foreign.rs, BYTES_BUFFER,
//! foreign.rs:371): write, write_byte, write_rune, write_string, write_to,
//! read, read_byte, read_bytes, read_from, read_rune, read_string, next,
//! grow, reset, string, bytes, len, cap, truncate, unread_byte, unread_rune.
//! No dedicated surface-test file existed before this draft.

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
    // Positive control: each declared method name must NOT produce HYB-E1002.
    for m in [
        "write", "write_byte", "write_rune", "write_string", "write_to",
        "read", "read_byte", "read_bytes", "read_from", "read_rune",
        "read_string", "next", "grow", "reset", "string", "bytes", "len",
        "cap", "truncate", "unread_byte", "unread_rune",
    ] {
        let ok = check(&format!(
            "import \"bytes\"\nfn inspect(b *bytes.buffer) {{\n    b.{m}()\n}}\n"
        ));
        assert!(
            !codes(&ok).iter().any(|c| c == "HYB-E1002"),
            "declared method {m} must not be flagged HYB-E1002; got {:?}",
            codes(&ok)
        );
    }
}
