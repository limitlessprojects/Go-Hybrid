//! Regression tests for Issue #4153:
//! [soundness] hy-check(buildable): curated net package functions bypass call arity
//! validation and package types/variables/constants are accepted as call targets
//! (HYB-E1002, Row 13).
//!
//! Under GUARANTEES.md Row 13 (BUILDABLE):
//! Any program accepted by `hyb check` must emit Go code that compiles under `go build`.

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
    assert!(
        cs.is_empty(),
        "expected accept for {:?}, got errors: {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Accept, "verdict for {:?}", src);
}

#[test]
fn test_net_package_call_too_few_arguments_rejected() {
    // net.parse_ip takes 1 argument, called with 0
    rejects_e1002(
        r#"
package main

import "net"

fn test() {
    _ = net.parse_ip()
}
"#,
    );

    // net.dial takes 2 arguments, called with 1
    rejects_e1002(
        r#"
package main

import "net"

fn test() {
    _, _ = net.dial("tcp")
}
"#,
    );

    // net.listen takes 2 arguments, called with 1
    rejects_e1002(
        r#"
package main

import "net"

fn test() {
    _, _ = net.listen("tcp")
}
"#,
    );

    // net.join_host_port takes 2 arguments, called with 1
    rejects_e1002(
        r#"
package main

import "net"

fn test() {
    _ = net.join_host_port("localhost")
}
"#,
    );

    // net.split_host_port takes 1 argument, called with 0
    rejects_e1002(
        r#"
package main

import "net"

fn test() {
    _, _, _ = net.split_host_port()
}
"#,
    );

    // net.cidr_mask takes 2 arguments, called with 1
    rejects_e1002(
        r#"
package main

import "net"

fn test() {
    _ = net.cidr_mask(24)
}
"#,
    );

    // net.dial_timeout takes 3 arguments, called with 2
    rejects_e1002(
        r#"
package main

import "net"

fn test() {
    _, _ = net.dial_timeout("tcp", "127.0.0.1:80")
}
"#,
    );

    // net.ipv4 takes 4 arguments, called with 3
    rejects_e1002(
        r#"
package main

import "net"

fn test() {
    _ = net.ipv4(127, 0, 0)
}
"#,
    );

    // net.ipv4_mask takes 4 arguments, called with 3
    rejects_e1002(
        r#"
package main

import "net"

fn test() {
    _ = net.ipv4_mask(255, 255, 255)
}
"#,
    );
}

#[test]
fn test_net_package_call_too_many_arguments_rejected() {
    // net.pipe takes 0 arguments, called with 1
    rejects_e1002(
        r#"
package main

import "net"

fn test() {
    _, _ = net.pipe("extra")
}
"#,
    );

    // net.interfaces takes 0 arguments, called with 1
    rejects_e1002(
        r#"
package main

import "net"

fn test() {
    _, _ = net.interfaces("extra")
}
"#,
    );

    // net.interface_addrs takes 0 arguments, called with 1
    rejects_e1002(
        r#"
package main

import "net"

fn test() {
    _, _ = net.interface_addrs("extra")
}
"#,
    );

    // net.parse_ip takes 1 argument, called with 2
    rejects_e1002(
        r#"
package main

import "net"

fn test() {
    _ = net.parse_ip("127.0.0.1", "extra")
}
"#,
    );

    // net.split_host_port takes 1 argument, called with 2
    rejects_e1002(
        r#"
package main

import "net"

fn test() {
    _, _, _ = net.split_host_port("localhost:80", "extra")
}
"#,
    );

    // net.dial takes 2 arguments, called with 3
    rejects_e1002(
        r#"
package main

import "net"

fn test() {
    _, _ = net.dial("tcp", "127.0.0.1:80", "extra")
}
"#,
    );

    // net.dial_timeout takes 3 arguments, called with 4
    rejects_e1002(
        r#"
package main

import "net"

fn test() {
    _, _ = net.dial_timeout("tcp", "127.0.0.1:80", 1000, "extra")
}
"#,
    );

    // net.ipv4 takes 4 arguments, called with 5
    rejects_e1002(
        r#"
package main

import "net"

fn test() {
    _ = net.ipv4(127, 0, 0, 1, 2)
}
"#,
    );
}

#[test]
fn test_net_non_callable_members_rejected() {
    let non_callable = [
        "net.conn()",
        "net.listener()",
        "net.packet_conn()",
        "net.addr()",
        "net.dialer()",
        "net.listen_config()",
        "net.resolver()",
        "net.ip()",
        "net.ip_mask()",
        "net.ip_net()",
        "net.ip_addr()",
        "net.ip_conn()",
        "net.tcp_addr()",
        "net.tcp_conn()",
        "net.tcp_listener()",
        "net.udp_addr()",
        "net.udp_conn()",
        "net.unix_addr()",
        "net.unix_conn()",
        "net.unix_listener()",
        "net.hardware_addr()",
        "net.buffers()",
        "net.addr_error()",
        "net.dns_config_error()",
        "net.dns_error()",
        "net.invalid_addr_error()",
        "net.op_error()",
        "net.parse_error()",
        "net.unknown_network_error()",
        "net.err_closed()",
        "net.err_write_to_connected()",
        "net.ipv4len()",
        "net.ipv6len()",
        "net.default_resolver()",
        "net.flags()",
    ];

    for call_expr in non_callable {
        let code = format!(
            r#"
package main

import "net"

fn test() {{
    _ = {}
}}
"#,
            call_expr
        );
        rejects_e1002(&code);
    }
}

#[test]
fn test_net_variadic_spread_on_non_variadic_rejected() {
    // net.dial is non-variadic; spread `...` must be rejected
    rejects_e1002(
        r#"
package main

import "net"

fn test(args []string) {
    _, _ = net.dial(args...)
}
"#,
    );

    // net.parse_ip is non-variadic; spread `...` must be rejected
    rejects_e1002(
        r#"
package main

import "net"

fn test(args []string) {
    _ = net.parse_ip(args...)
}
"#,
    );
}

#[test]
fn test_net_positive_controls_accepted() {
    accepts(
        r#"
package main

import "net"

fn test() {
    _, _ = net.pipe()
    _, _ = net.interfaces()
    _, _ = net.interface_addrs()
    _ = net.parse_ip("127.0.0.1")
    _ = net.ipv4(127, 0, 0, 1)
    _ = net.ipv4_mask(255, 255, 255, 0)
    _ = net.cidr_mask(24, 32)
    _ = net.join_host_port("localhost", "8080")
    _, _, _ = net.split_host_port("localhost:8080")
    _, _ = net.dial("tcp", "127.0.0.1:80")
    _, _ = net.listen("tcp", "127.0.0.1:8080")
    _, _ = net.dial_timeout("tcp", "127.0.0.1:80", 1000)
}
"#,
    );
}

#[test]
fn test_shadowed_net_not_interfered() {
    accepts(
        r#"
package main

struct mock_net {
    conn fn() int
}

fn test(net mock_net) {
    _ = net.conn()
}
"#,
    );
}
