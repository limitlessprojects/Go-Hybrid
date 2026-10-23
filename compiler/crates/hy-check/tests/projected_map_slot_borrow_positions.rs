use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    check_file(&parse_file("inline.hy", src).expect("must parse"))
}

#[test]
fn projected_map_element_borrows_reject_e3104() {
    let reject_cases = [
        r#"struct acct { bal int }
fn touch(p *int) { *p = 9 }
fn main() {
    m := map[string]acct{"a": acct{bal: 1}}
    touch(&m["a"].bal)
}
"#,
    ];
    for src in reject_cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected REJECT for:
{}", src);
        assert!(r.diagnostics.iter().any(|d| d.severity == Severity::Error && d.code == "HYB-E3104"),
                "expected HYB-E3104 for:
{}
diags: {:?}", src, r.diagnostics);
    }
}
