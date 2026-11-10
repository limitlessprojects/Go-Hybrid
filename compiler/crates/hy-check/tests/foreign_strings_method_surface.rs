//! Tests for the curated foreign method surface of `strings.builder`.
//! Guarantees that calling phantom / unsupported methods on the curated
//! `strings.builder` foreign surface rejects fail-closed with HYB-E1002, and
//! every declared method name is accepted outright (Row 13 BUILDABLE).
//!
//! Surface under test (declared in hy-check/src/foreign.rs, STRINGS_BUILDER):
//! write, write_byte, write_rune, write_string, grow, reset, string, len, cap.

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn phantom_builder_methods_are_rejected() {
    let bad = check(
        "import \"strings\"\n\
         fn inspect(b *strings.builder) {\n\
             b.totally_fake()\n\
         }\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject);
    assert!(codes(&bad).iter().any(|c| c == "HYB-E1002"));
    let diag = bad.diagnostics.iter().find(|d| d.code == "HYB-E1002").unwrap();
    assert!(diag.message.contains("strings.builder"), "{}", diag.message);
}

#[test]
fn every_declared_builder_method_is_accepted() {
    // Positive control: each declared method name must be accepted outright.
    for m in [
        "write", "write_byte", "write_rune", "write_string", "grow",
        "reset", "string", "len", "cap",
    ] {
        let ok = check(&format!(
            "import \"strings\"\nfn inspect(b *strings.builder) {{\n    b.{m}()\n}}\n"
        ));
        assert_eq!(
            ok.verdict,
            Verdict::Accept,
            "declared method {m} must be accepted; got {:?}",
            codes(&ok)
        );
    }
}
