//! A `select` with more than one `default` arm passes the checker today and
//! emits two `default:` clauses, which `go build` rejects with
//! `multiple defaults`. Row 13 (BUILDABLE).

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {diags:?}"));
    check_file(&ast)
}

fn codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn multiple_defaults_rejected() {
    let r = check(
        r#"fn main() {
    select {
        default: { fmt.println(0) }
        default: { fmt.println(1) }
    }
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    let found = codes(&r);
    assert_eq!(
        found,
        vec!["HYB-E1002".to_string()],
        "expected one HYB-E1002, got {found:?}"
    );
}

#[test]
fn single_default_with_recv_arm_accepted() {
    let r = check(
        r#"fn main() {
    ch := make(chan int, 1)
    select {
        v := <-ch: { fmt.println(v) }
        default: { fmt.println(0) }
    }
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Accept, "diagnostics: {:?}", codes(&r));
}

#[test]
fn single_default_with_send_arm_accepted() {
    let r = check(
        r#"fn main() {
    ch := make(chan int, 1)
    select {
        ch <- 1: { }
        default: { }
    }
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Accept, "diagnostics: {:?}", codes(&r));
}
