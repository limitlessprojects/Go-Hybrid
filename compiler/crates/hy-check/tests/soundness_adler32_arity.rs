//! Regression test for #4317:
//! hash/adler32 package functions bypass call arity validation and package constant is accepted as call target (HYB-E1002, Row 13)

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
fn test_adler32_call_arity_and_constant_call_rejected() {
    // Positive cases accepted:
    accepts("import \"hash/adler32\"\nfn main() {\n    h := adler32.new()\n    _ = h\n}\n");
    accepts("import \"hash/adler32\"\nfn main() {\n    data := make([]byte, 4)\n    sum := adler32.checksum(data)\n    _ = sum\n}\n");

    // Invalid arities rejected:
    rejects_e1002("import \"hash/adler32\"\nfn main() {\n    h := adler32.new(1)\n    _ = h\n}\n");
    rejects_e1002("import \"hash/adler32\"\nfn main() {\n    sum := adler32.checksum()\n    _ = sum\n}\n");
    rejects_e1002("import \"hash/adler32\"\nfn main() {\n    data := make([]byte, 4)\n    sum := adler32.checksum(data, 1)\n    _ = sum\n}\n");

    // Constant invocation rejected:
    rejects_e1002("import \"hash/adler32\"\nfn main() {\n    s := adler32.size()\n    _ = s\n}\n");
    rejects_e1002("import \"hash/adler32\"\nfn main() {\n    s := adler32.size(1)\n    _ = s\n}\n");
}
