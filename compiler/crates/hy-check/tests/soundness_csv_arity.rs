//! Regression test for csv call arity and symbol validation (HYB-E1002)

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
fn test_csv_call_arity_and_symbols() {
    // Positive cases accepted:
    accepts("import \"encoding/csv\"\nfn main() {\n    r := csv.new_reader(nil)\n    _ = r\n}");
    accepts("import \"encoding/csv\"\nfn main() {\n    w := csv.new_writer(nil)\n    _ = w\n}");

    // Invalid arities rejected:
    rejects_e1002("import \"encoding/csv\"\nfn main() {\n    r := csv.new_reader()\n    _ = r\n}");
    rejects_e1002("import \"encoding/csv\"\nfn main() {\n    r := csv.new_reader(nil, 1)\n    _ = r\n}");

    // Constant / Type invocation rejected:
    rejects_e1002("import \"encoding/csv\"\nfn main() {\n    x := csv.reader()\n    _ = x\n}");
    rejects_e1002("import \"encoding/csv\"\nfn main() {\n    x := csv.err_bare_quote()\n    _ = x\n}");
}
