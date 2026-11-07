//! Regression tests for foreign method surface gating on `net.op_error`,
//! `net.dns_error`, `net.dns_config_error`, `net.addr_error`,
//! `net.invalid_addr_error`, `net.parse_error`, `net.unknown_network_error`,
//! `net.hardware_addr`, and `net.interface` (`HYB-E1002`, Row 13 BUILDABLE).
//!
//! Under `GUARANTEES.md` Row 13 (`BUILDABLE`), every program accepted by `hyb check`
//! must emit valid Go that builds with `go build`.
//! Calling a phantom or non-existent method on these curated `net` package types
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
fn test_net_op_error_phantom_methods_reject() {
    let src = r#"
import "net"

struct state {
    oe net.op_error
}

fn main() {
    var s state
    s.oe.totally_fake()
}
"#;
    rejects_e1002(src, "net.op_error");
}

#[test]
fn test_net_op_error_valid_methods_accept() {
    let src = r#"
import "net"

struct state {
    oe net.op_error
}

fn main() {
    var s state
    _ = s.oe.error()
    _ = s.oe.temporary()
    _ = s.oe.timeout()
    _ = s.oe.unwrap()
}
"#;
    accepts(src);
}

#[test]
fn test_net_dns_and_addr_errors_phantom_methods_reject() {
    let src = r#"
import "net"

struct state {
    de net.dns_error
}

fn main() {
    var s state
    s.de.totally_fake()
}
"#;
    rejects_e1002(src, "net.dns_error");
}

#[test]
fn test_net_dns_and_addr_errors_valid_methods_accept() {
    let src = r#"
import "net"

struct state {
    de net.dns_error
    dce net.dns_config_error
    ae net.addr_error
    iae net.invalid_addr_error
    pe net.parse_error
    une net.unknown_network_error
}

fn main() {
    var s state
    _ = s.de.error()
    _ = s.de.temporary()
    _ = s.de.timeout()
    _ = s.de.unwrap()
    _ = s.dce.error()
    _ = s.dce.temporary()
    _ = s.dce.timeout()
    _ = s.dce.unwrap()
    _ = s.ae.error()
    _ = s.ae.temporary()
    _ = s.ae.timeout()
    _ = s.iae.error()
    _ = s.iae.temporary()
    _ = s.iae.timeout()
    _ = s.pe.error()
    _ = s.pe.temporary()
    _ = s.pe.timeout()
    _ = s.une.error()
    _ = s.une.temporary()
    _ = s.une.timeout()
}
"#;
    accepts(src);
}

#[test]
fn test_net_hardware_addr_and_interface_phantom_methods_reject() {
    let src = r#"
import "net"

struct state {
    hw net.hardware_addr
}

fn main() {
    var s state
    s.hw.totally_fake()
}
"#;
    rejects_e1002(src, "net.hardware_addr");
}

#[test]
fn test_net_hardware_addr_and_interface_valid_methods_accept() {
    let src = r#"
import "net"

struct state {
    hw net.hardware_addr
    iface net.interface
}

fn main() {
    var s state
    _ = s.hw.string()
    s.iface.addrs()
    s.iface.multicast_addrs()
}
"#;
    accepts(src);
}
