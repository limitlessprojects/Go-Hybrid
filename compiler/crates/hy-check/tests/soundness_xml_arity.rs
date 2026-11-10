//! Regression test for xml call arity and symbol validation (HYB-E1002)

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
fn test_xml_call_arity_and_symbols() {
    // Positive cases accepted:
    accepts("import \"encoding/xml\"\nfn main() {\n    b, err := xml.marshal(nil)\n    _ = b\n    _ = err\n}");
    accepts("import \"encoding/xml\"\nfn main() {\n    dec := xml.new_decoder(nil)\n    _ = dec\n}");

    // Invalid arities rejected:
    rejects_e1002("import \"encoding/xml\"\nfn main() {\n    b, err := xml.marshal()\n    _ = b\n    _ = err\n}");
    rejects_e1002("import \"encoding/xml\"\nfn main() {\n    b, err := xml.marshal(nil, 1)\n    _ = b\n    _ = err\n}");

    // Constant / Type invocation rejected:
    rejects_e1002("import \"encoding/xml\"\nfn main() {\n    x := xml.decoder()\n    _ = x\n}");
    rejects_e1002("import \"encoding/xml\"\nfn main() {\n    x := xml.header()\n    _ = x\n}");
}
