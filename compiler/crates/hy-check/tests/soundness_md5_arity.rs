//! Regression test for md5 call arity and symbol validation (HYB-E1002)

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
fn test_md5_call_arity_and_symbols() {
    // Positive cases accepted:
    accepts("import \"crypto/md5\"\nfn main() {\n    h := md5.new()\n    _ = h\n}");
    accepts("import \"crypto/md5\"\nfn main() {\n    data := make([]byte, 4)\n    s := md5.sum(data)\n    _ = s\n}");

    // Invalid arities rejected:
    rejects_e1002("import \"crypto/md5\"\nfn main() {\n    h := md5.new(1)\n    _ = h\n}");
    rejects_e1002("import \"crypto/md5\"\nfn main() {\n    s := md5.sum()\n    _ = s\n}");

    // Constant / Type invocation rejected:
    rejects_e1002("import \"crypto/md5\"\nfn main() {\n    s := md5.size()\n    _ = s\n}");
    rejects_e1002("import \"crypto/md5\"\nfn main() {\n    b := md5.block_size()\n    _ = b\n}");
}
