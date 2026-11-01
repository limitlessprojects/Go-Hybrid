//! Regression tests for #1343: string-index assignment/loan (`s[0] = v`, `&s[0]`,
//! including through `match` option binders) must reject as HYB-E2010.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects_with(src: &str, code: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == code),
        "expected {}, got {:?}\nsrc:\n{}",
        code,
        cs,
        src
    );
    assert_eq!(r.verdict, Verdict::Reject, "src:\n{}", src);
}

#[test]
fn rejects_string_index_store_and_loan_including_match_option_binder() {
    rejects_with(
        "fn main() {\n\
            s := \"hello\"\n\
            s[0] = 65\n\
            n := &s[0]\n\
            _ = n\n\
            _ = s\n\
        }\n",
        "HYB-E2010",
    );
    rejects_with(
        "fn main() {\n\
            var opt option[string] = some(\"hello\")\n\
            match opt {\n\
                some(s): {\n\
                    s[0] = 65\n\
                    n := &s[0]\n\
                    _ = n\n\
                    _ = s\n\
                }\n\
                none: {}\n\
            }\n\
        }\n",
        "HYB-E2010",
    );
}
