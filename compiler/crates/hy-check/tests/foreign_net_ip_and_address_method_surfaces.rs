//! Regression tests for foreign method surface gating on `net.ip`, `net.ip_net`,
//! `net.ip_mask`, `net.tcp_addr`, `net.udp_addr`, `net.unix_addr`, and `net.ip_addr`
//! handles (`HYB-E1002`, Row 13 BUILDABLE).
//!
//! Under GUARANTEES.md Row 13 (BUILDABLE), every program accepted by `hyb check`
//! must emit valid Go that builds with `go build`.
//! Calling a phantom or non-existent method on these standard library network address handles passed
//! `hyb check` with 0 diagnostics and died at `go build` with
//! `type net.TCPAddr has no field or method ...`.

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
fn test_net_ip_phantom_methods_reject() {
    let src = r#"
import "net"

struct state {
    ip net.ip
}

fn main() {
    var s state
    s.ip.totally_fake()
}
"#;
    rejects_e1002(src, "net.ip");
}

#[test]
fn test_net_ip_net_phantom_methods_reject() {
    let src = r#"
import "net"

struct state {
    n net.ip_net
}

fn main() {
    var s state
    s.n.bogus_call()
}
"#;
    rejects_e1002(src, "net.ip_net");
}

#[test]
fn test_net_ip_mask_phantom_methods_reject() {
    let src = r#"
import "net"

struct state {
    m net.ip_mask
}

fn main() {
    var s state
    s.m.fake_method()
}
"#;
    rejects_e1002(src, "net.ip_mask");
}

#[test]
fn test_net_tcp_addr_phantom_methods_reject() {
    let src = r#"
import "net"

struct state {
    addr net.tcp_addr
}

fn main() {
    var s state
    s.addr.non_existent()
}
"#;
    rejects_e1002(src, "net.tcp_addr");
}

#[test]
fn test_net_udp_addr_phantom_methods_reject() {
    let src = r#"
import "net"

struct state {
    addr net.udp_addr
}

fn main() {
    var s state
    s.addr.non_existent()
}
"#;
    rejects_e1002(src, "net.udp_addr");
}

#[test]
fn test_net_unix_addr_phantom_methods_reject() {
    let src = r#"
import "net"

struct state {
    addr net.unix_addr
}

fn main() {
    var s state
    s.addr.non_existent()
}
"#;
    rejects_e1002(src, "net.unix_addr");
}

#[test]
fn test_net_ip_addr_phantom_methods_reject() {
    let src = r#"
import "net"

struct state {
    addr net.ip_addr
}

fn main() {
    var s state
    s.addr.non_existent()
}
"#;
    rejects_e1002(src, "net.ip_addr");
}

#[test]
fn test_net_ip_pointer_receiver_phantom_rejects() {
    let src = r#"
import "net"

fn inspect(ip *net.ip) {
    ip.bogus_call()
}
"#;
    rejects_e1002(src, "net.ip");
}

#[test]
fn test_net_ip_and_address_valid_methods_accept() {
    let src = r#"
import "net"

struct state {
    ip net.ip
    n net.ip_net
    m net.ip_mask
    tcp net.tcp_addr
    udp net.udp_addr
    un net.unix_addr
    ipa net.ip_addr
}

fn main() {
    var s state
    _ = s.ip.is_loopback()
    _ = s.ip.string()
    _ = s.n.network()
    _ = s.m.size()
    _ = s.tcp.network()
    _ = s.udp.network()
    _ = s.un.network()
    _ = s.ipa.network()
}
"#;
    accepts(src);
}

#[test]
fn test_net_ip_pointer_valid_methods_accept() {
    let src = r#"
import "net"

fn inspect(ip *net.ip, tcp *net.tcp_addr) {
    _ = ip.is_loopback()
    _ = tcp.network()
}
"#;
    accepts(src);
}
