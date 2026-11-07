//! Regression tests for foreign method surface gating on `strings.replacer`,
//! `url.userinfo`, `net.listen_config`, and `http.response_controller`
//! (`HYB-E1002`, Row 13 BUILDABLE).
//!
//! Under `GUARANTEES.md` Row 13 (`BUILDABLE`), every program accepted by `hyb check`
//! must emit valid Go that builds with `go build`.
//! Calling a phantom or non-existent method on these curated standard library handles
//! passed `hyb check` with 0 diagnostics and failed at `go build` with
//! `type ... has no field or method ...`.

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

fn rejects_e1002(src: &str, expected_substring: &str) {
    let r = check(src);
    let codes = error_codes(&r);
    assert!(
        codes.contains(&"HYB-E1002".to_string()),
        "must emit HYB-E1002, got: {:?} for\n{}",
        codes,
        src
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.message.contains(expected_substring)),
        "diagnostic message must contain {:?}, got: {:?}",
        expected_substring,
        r.diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

fn accepts(src: &str) {
    let r = check(src);
    let codes = error_codes(&r);
    assert!(
        codes.is_empty(),
        "must accept with 0 errors, got: {:?} for\n{}",
        codes,
        src
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn test_strings_replacer_phantom_methods_reject() {
    let src = r#"
import "strings"

struct state {
    rep strings.replacer
}

fn main() {
    var s state
    s.rep.totally_fake()
}
"#;
    rejects_e1002(src, "strings.replacer");
}

#[test]
fn test_strings_new_replacer_local_phantom_methods_reject() {
    let src = r#"
import "strings"

fn main() {
    rep := strings.new_replacer("a", "b")
    rep.non_existent()
}
"#;
    rejects_e1002(src, "strings.replacer");
}

#[test]
fn test_strings_replacer_valid_methods_accept() {
    let src = r#"
import "strings"

fn main() {
    rep := strings.new_replacer("a", "b")
    out := rep.replace("abc")
    _ = out
}
"#;
    accepts(src);
}

#[test]
fn test_url_userinfo_phantom_methods_reject() {
    let src = r#"
import "net/url"

struct state {
    u url.userinfo
}

fn main() {
    var s state
    s.u.totally_fake()
}
"#;
    rejects_e1002(src, "url.userinfo");
}

#[test]
fn test_url_user_local_phantom_methods_reject() {
    let src = r#"
import "net/url"

fn main() {
    u := url.user("alice")
    u.reset()
}
"#;
    rejects_e1002(src, "url.userinfo");
}

#[test]
fn test_url_userinfo_valid_methods_accept() {
    let src = r#"
import "net/url"

fn main() {
    u := url.user("alice")
    name := u.username()
    s := u.string()
    _ = name
    _ = s
}
"#;
    accepts(src);
}

#[test]
fn test_net_listen_config_phantom_methods_reject() {
    let src = r#"
import "net"

struct state {
    lc net.listen_config
}

fn main() {
    var s state
    s.lc.totally_fake()
}
"#;
    rejects_e1002(src, "net.listen_config");
}

#[test]
fn test_http_response_controller_phantom_methods_reject() {
    let src = r#"
import "net/http"

struct state {
    rc http.response_controller
}

fn main() {
    var s state
    s.rc.totally_fake()
}
"#;
    rejects_e1002(src, "http.response_controller");
}

#[test]
fn test_http_response_controller_valid_methods_accept() {
    let src = r#"
import "net/http"

struct state {
    rc http.response_controller
}

fn main() {
    var s state
    s.rc.flush()
    s.rc.enable_full_duplex()
}
"#;
    accepts(src);
}
