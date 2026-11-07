//! Regression tests for foreign method surface gating on `net.conn`, `net.listener`,
//! `net.packet_conn`, `net.dialer`, `net.tcp_listener`, `net.tcp_conn`, and `net.udp_conn` handles
//! (`HYB-E1002`, Row 13 BUILDABLE).
//!
//! Under GUARANTEES.md Row 13 (BUILDABLE), every program accepted by `hyb check`
//! must emit valid Go that builds with `go build`.
//! Calling a phantom or non-existent method on these standard library networking handles passed
//! `hyb check` with 0 diagnostics and died at `go build` with
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
fn test_net_conn_phantom_method_rejects() {
    let src = r#"
import "net"

struct state {
    c net.conn
}

fn main() {
    var s state
    s.c.totally_fake()
}
"#;
    rejects_e1002(src, "net.conn");
}

#[test]
fn test_net_listener_phantom_method_rejects() {
    let src = r#"
import "net"

struct state {
    ln net.listener
}

fn main() {
    var s state
    s.ln.invalid_method()
}
"#;
    rejects_e1002(src, "net.listener");
}

#[test]
fn test_net_packet_conn_phantom_method_rejects() {
    let src = r#"
import "net"

struct state {
    pc net.packet_conn
}

fn main() {
    var s state
    s.pc.bogus_call()
}
"#;
    rejects_e1002(src, "net.packet_conn");
}

#[test]
fn test_net_dialer_phantom_method_rejects() {
    let src = r#"
import "net"

struct state {
    d net.dialer
}

fn main() {
    var s state
    s.d.fake_dial()
}
"#;
    rejects_e1002(src, "net.dialer");
}

#[test]
fn test_net_tcp_listener_phantom_method_rejects() {
    let src = r#"
import "net"

struct state {
    tl net.tcp_listener
}

fn main() {
    var s state
    s.tl.unsupported_call()
}
"#;
    rejects_e1002(src, "net.tcp_listener");
}

#[test]
fn test_net_tcp_conn_phantom_method_rejects() {
    let src = r#"
import "net"

struct state {
    tc net.tcp_conn
}

fn main() {
    var s state
    s.tc.wrong_method()
}
"#;
    rejects_e1002(src, "net.tcp_conn");
}

#[test]
fn test_net_udp_conn_phantom_method_rejects() {
    let src = r#"
import "net"

struct state {
    uc net.udp_conn
}

fn main() {
    var s state
    s.uc.bad_method()
}
"#;
    rejects_e1002(src, "net.udp_conn");
}

#[test]
fn test_real_methods_on_net_handles_accept() {
    let src = r#"
import "net"

struct state {
    conn         net.conn
    ln           net.listener
    dialer       net.dialer
    packet_conn  net.packet_conn
    tcp_ln       net.tcp_listener
    tcp_conn     net.tcp_conn
    udp_conn     net.udp_conn
}

fn main() {
    var s state
    _ = s.conn.close()
    _ = s.ln.close()
    _ = s.packet_conn.close()
    _ = s.tcp_ln.close()
    _ = s.tcp_conn.close()
    _ = s.udp_conn.close()
}
"#;
    accepts(src);
}