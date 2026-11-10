//! Regression test for crc32 call arity and symbol validation (HYB-E1002)

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
        "expected HYB-E1002 for {:?}, got {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "verdict for {:?}: {:?}", src, r.diagnostics);
}

#[test]
fn test_crc32_call_arity_and_symbols() {
    // Positive cases accepted:
    accepts("import \"hash/crc32\"\nfn main() {\n    h := crc32.new_ieee()\n    _ = h\n}");
    accepts("import \"hash/crc32\"\nfn main() {\n    data := make([]byte, 4)\n    s := crc32.checksum_ieee(data)\n    _ = s\n}");

    // Invalid arities rejected:
    rejects_e1002("import \"hash/crc32\"\nfn main() {\n    h := crc32.new_ieee(1)\n    _ = h\n}");
    rejects_e1002("import \"hash/crc32\"\nfn main() {\n    s := crc32.checksum_ieee()\n    _ = s\n}");

    // Constant / Type invocation rejected:
    rejects_e1002("import \"hash/crc32\"\nfn main() {\n    s := crc32.size()\n    _ = s\n}");
    rejects_e1002("import \"hash/crc32\"\nfn main() {\n    t := crc32.table()\n    _ = t\n}");
}
