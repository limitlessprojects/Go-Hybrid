//! Tests for the curated foreign method surface of `flate2.writer`.
//! The flate2 surface is deliberately unbound (UNBOUND_RUST_CRATES), but its
//! method surface stays pinned so the checker does not invent names. This
//! battery pins both directions per Row 13: phantom methods reject fail-closed
//! with HYB-E1002, and every declared method name is accepted.

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn phantom_flate2_writer_methods_are_rejected() {
    let bad = check(
        "import \"flate2\"\n\
         fn compress(w *flate2.writer) {\n\
             w.totally_fake()\n\
         }\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject);
    assert!(codes(&bad).iter().any(|c| c == "HYB-E1002"));
    let diag = bad
        .diagnostics
        .iter()
        .find(|d| d.code == "HYB-E1002")
        .unwrap();
    assert!(diag.message.contains("flate2.writer"), "{}", diag.message);
}

#[test]
fn every_declared_flate2_writer_method_is_accepted() {
    // Over-restriction guard: the unbound surface still accepts its two pinned names.
    for m in ["write", "finish"] {
        let ok = check(&format!(
            "import \"flate2\"\nfn compress(w *flate2.writer) {{\n w.{m}()\n}}\n"
        ));
        assert_eq!(
            ok.verdict,
            Verdict::Accept,
            "declared method {m} must be accepted; got {:?}",
            codes(&ok)
        );
    }
}
