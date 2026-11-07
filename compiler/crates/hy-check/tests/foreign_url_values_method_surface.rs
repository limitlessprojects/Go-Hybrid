//! Tests for curated foreign method surface of `url.values` (HYB-E1002, Row 13 BUILDABLE).
//! Guarantees that calling phantom/unsupported methods on `url.values` or `*url.values`
//! rejects fail-closed with HYB-E1002, while all real Go standard library methods accept cleanly.

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn phantom_url_values_methods_are_rejected() {
    let bad = check(
        "import \"url\"\n         struct holder {\n             v url.values\n         }\n         fn main() {\n             var h holder\n             h.v.totally_fake_method()\n             fmt.println(1)\n         }\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject, "expected reject, got {:?}", bad.diagnostics);
    assert!(
        codes(&bad).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        codes(&bad)
    );
}

#[test]
fn pointer_url_values_phantom_method_is_rejected() {
    let bad = check(
        "import \"url\"\n         fn check_ptr(v *url.values) {\n             v.unknown_method()\n             fmt.println(1)\n         }\n         fn main() {}\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject, "expected reject, got {:?}", bad.diagnostics);
    assert!(
        codes(&bad).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        codes(&bad)
    );
}

#[test]
fn real_url_values_methods_accept() {
    let good = check(
        "import \"url\"\n         struct holder {\n             v url.values\n         }\n         fn main() {\n             var h holder\n             h.v.add(\"k\", \"v\")\n             h.v.set(\"k\", \"v2\")\n             _ = h.v.get(\"k\")\n             _ = h.v.has(\"k\")\n             _ = h.v.encode()\n             h.v.del(\"k\")\n             fmt.println(1)\n         }\n",
    );
    assert_eq!(good.verdict, Verdict::Accept, "expected accept, got {:?}", good.diagnostics);
}

#[test]
fn query_chain_url_values_methods_accept() {
    let good = check(
        "import \"url\"\n         fn test_chain(u url.url) {\n             var q = u.query()\n             _ = q.get(\"param\")\n             _ = q.encode()\n             fmt.println(1)\n         }\n         fn main() {}\n",
    );
    assert_eq!(good.verdict, Verdict::Accept, "expected accept, got {:?}", good.diagnostics);
}
