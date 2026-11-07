//! Regression tests for foreign method surface gating on 
et.unix_conn, 
et.ip_conn,
//! 
et.unix_listener, 
et.resolver, and 
et.buffers handles (HYB-E1002, Row 13 BUILDABLE).
//!
//! Under GUARANTEES.md Row 13 (BUILDABLE), every program accepted by hyb check
//! must emit valid Go that builds with go build.
//! Calling a phantom or non-existent method on these standard library network handles passed
//! hyb check with 0 diagnostics and died at go build with
//! 	ype *net.UnixConn has no field or method ....

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
fn test_net_unix_conn_phantom_methods_reject() {
    let src = r#"
import "net"

struct state {
    uc net.unix_conn
}

fn main() {
    var s state
    s.uc.totally_fake()
}
"#;
    rejects_e1002(src, "net.unix_conn");
}

#[test]
fn test_net_unix_conn_valid_methods_accept() {
    let src = r#"
import "net"

struct state {
    uc net.unix_conn
}

fn main() {
    var s state
    s.uc.close()
    s.uc.close_read()
    s.uc.close_write()
}
"#;
    accepts(src);
}

#[test]
fn test_net_ip_conn_phantom_methods_reject() {
    let src = r#"
import "net"

struct state {
    ic net.ip_conn
}

fn main() {
    var s state
    s.ic.bogus_operation()
}
"#;
    rejects_e1002(src, "net.ip_conn");
}

#[test]
fn test_net_ip_conn_valid_methods_accept() {
    let src = r#"
import "net"

struct state {
    ic net.ip_conn
}

fn main() {
    var s state
    s.ic.close()
    s.ic.local_addr()
    s.ic.remote_addr()
}
"#;
    accepts(src);
}

#[test]
fn test_net_unix_listener_phantom_methods_reject() {
    let src = r#"
import "net"

struct state {
    ul net.unix_listener
}

fn main() {
    var s state
    s.ul.invalid_listen()
}
"#;
    rejects_e1002(src, "net.unix_listener");
}

#[test]
fn test_net_unix_listener_valid_methods_accept() {
    let src = r#"
import "net"

struct state {
    ul net.unix_listener
}

fn main() {
    var s state
    s.ul.addr()
    s.ul.close()
    s.ul.set_unlink_on_close(true)
}
"#;
    accepts(src);
}

#[test]
fn test_net_resolver_phantom_methods_reject() {
    let src = r#"
import "net"

struct state {
    res net.resolver
}

fn main() {
    var s state
    s.res.bogus_dns_lookup()
}
"#;
    rejects_e1002(src, "net.resolver");
}

#[test]
fn test_net_resolver_valid_methods_accept() {
    let src = r#"
import "net"

struct state {
    res net.resolver
}

fn main() {
    var s state
    s.res.lookup_host("example.com")
    s.res.lookup_ip("example.com")
}
"#;
    accepts(src);
}

#[test]
fn test_net_buffers_phantom_methods_reject() {
    let src = r#"
import "net"

struct state {
    buf net.buffers
}

fn main() {
    var s state
    s.buf.not_a_method()
}
"#;
    rejects_e1002(src, "net.buffers");
}

#[test]
fn test_net_buffers_valid_methods_accept() {
    let src = r#"
import "net"

struct state {
    buf net.buffers
}

fn main() {
    var s state
    s.buf.read()
}
"#;
    accepts(src);
}