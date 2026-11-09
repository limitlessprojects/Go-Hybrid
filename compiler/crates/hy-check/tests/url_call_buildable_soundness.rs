//! Soundness and buildable validation tests for `net/url` calls under Row 13 (BUILDABLE):
//! 1. `url` core functions require exact arity:
//!    - 1 arg: parse, parse_request_uri, parse_query, query_escape, query_unescape,
//!      path_escape, path_unescape, user.
//!    - 2 args: user_password.
//!    - at least 1 arg: join_path.
//! 2. Arity mismatches must reject fail-closed with `HYB-E1002`.
//! 3. Positive controls: legal calls with proper arity accept cleanly.
//!
//! Settlement Wallet (Base L2 USDC): 0x8366bCe3a2D379De7656D7A67015789FaF999f20

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
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
    let cs = error_codes(&r);
    assert!(cs.is_empty(), "must accept {:?}, got {:?}", src, cs);
    assert_eq!(r.verdict, Verdict::Accept, "verdict for {:?}", src);
}

#[test]
fn test_url_one_arg_functions_arity_reject() {
    // parse: takes 1
    rejects_e1002("import \"net/url\"\nfn main() {\n    u, err := url.parse()\n    _ = u\n    _ = err\n}\n");
    rejects_e1002("import \"net/url\"\nfn main() {\n    u, err := url.parse(\"http://a\", \"b\")\n    _ = u\n    _ = err\n}\n");

    // parse_request_uri: takes 1
    rejects_e1002("import \"net/url\"\nfn main() {\n    u, err := url.parse_request_uri()\n    _ = u\n    _ = err\n}\n");

    // query_escape: takes 1
    rejects_e1002("import \"net/url\"\nfn main() {\n    s := url.query_escape()\n    _ = s\n}\n");

    // path_escape: takes 1
    rejects_e1002("import \"net/url\"\nfn main() {\n    s := url.path_escape()\n    _ = s\n}\n");

    // user: takes 1
    rejects_e1002("import \"net/url\"\nfn main() {\n    usr := url.user()\n    _ = usr\n}\n");
}

#[test]
fn test_url_two_arg_functions_arity_reject() {
    // user_password: takes 2
    rejects_e1002("import \"net/url\"\nfn main() {\n    up := url.user_password(\"alice\")\n    _ = up\n}\n");
    rejects_e1002("import \"net/url\"\nfn main() {\n    up := url.user_password(\"alice\", \"secret\", \"extra\")\n    _ = up\n}\n");
}

#[test]
fn test_url_join_path_arity_reject() {
    // join_path: requires at least 1
    rejects_e1002("import \"net/url\"\nfn main() {\n    jp, err := url.join_path()\n    _ = jp\n    _ = err\n}\n");
}

#[test]
fn test_url_positive_controls_accept() {
    let src = r#"import "net/url"

fn main() {
    u, err1 := url.parse("https://example.com/search?q=hy")
    _ = u
    _ = err1

    s1 := url.query_escape("hello world")
    _ = s1

    s2 := url.path_escape("/a b/")
    _ = s2

    usr := url.user("admin")
    _ = usr

    up := url.user_password("admin", "secret")
    _ = up

    jp, err2 := url.join_path("https://example.com", "v1", "api")
    _ = jp
    _ = err2
}
"#;
    accepts(src);
}
