//! Tests for the curated foreign method surface of `sync.map`.
//! Guarantees that calling phantom/unsupported methods rejects fail-closed with
//! HYB-E1002 (Row 13 BUILDABLE), and that every declared method name is
//! accepted (over-restriction guard). `sync.map` is deliberately omitted from
//! the main syntax battery (parser bare-map restriction), so these tests use
//! the `*sync.map` parameter form to stay inside the parser's envelope.

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn phantom_map_methods_are_rejected() {
    let bad = check(
        "import \"sync\"\n\
         fn load_it(m *sync.map) {\n\
             m.unknown_lookup()\n\
         }\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject);
    assert!(codes(&bad).iter().any(|c| c == "HYB-E1002"));
    let diag = bad
        .diagnostics
        .iter()
        .find(|d| d.code == "HYB-E1002")
        .unwrap();
    assert!(diag.message.contains("sync.map"), "{}", diag.message);
}

#[test]
fn every_declared_map_method_is_accepted() {
    // Over-restriction guard: all nine declared names must be accepted.
    for m in [
        "load",
        "store",
        "delete",
        "load_or_store",
        "load_and_delete",
        "range",
        "compare_and_swap",
        "compare_and_delete",
        "swap",
    ] {
        let ok = check(&format!(
            "import \"sync\"\nfn load_it(m *sync.map) {{\n m.{m}()\n}}\n"
        ));
        assert_eq!(
            ok.verdict,
            Verdict::Accept,
            "declared method {m} must be accepted; got {:?}",
            codes(&ok)
        );
    }
}
