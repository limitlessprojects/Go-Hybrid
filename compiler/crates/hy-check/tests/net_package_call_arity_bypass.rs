//! Regression tests for Wave 22:
//! Curated foreign package function calls (`net.*`) must enforce call arity
//! constraints and non-callable member checks during `hyb check` (HYB-E1002, Row 13 BUILDABLE).
//!
//! Without arity checks on these foreign package calls, invocations with too few or too many arguments,
//! or invocations on non-callable package types/variables/constants (`net.conn`, `net.listener`, `net.ip`, `net.ipv4len`, `net.err_closed`),
//! passed `hyb check` with 0 diagnostics and subsequently failed downstream `go build`.

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
    // net.conn is an interface type, not a function
    rejects_e1002(
        r#"
package main

import "net"

fn test() {
    _ = net.conn()
}
"#,
    );

    // net.listener is an interface type, not a function
    rejects_e1002(
        r#"
package main

import "net"

fn test() {
    _ = net.listener()
}
"#,
    );

    // net.ip is a type, not a function
    rejects_e1002(
        r#"
package main

import "net"

fn test() {
    _ = net.ip()
}
"#,
    );

    // net.tcp_addr is a struct type, not a function
    rejects_e1002(
        r#"
package main

import "net"

fn test() {
    _ = net.tcp_addr()
}
"#,
    );

    // net.ipv4len is an int constant, not a function
    rejects_e1002(
        r#"
package main

import "net"

fn test() {
    _ = net.ipv4len()
}
"#,
    );

    // net.err_closed is an error variable, not a function
    rejects_e1002(
        r#"
package main

import "net"

fn test() {
    _ = net.err_closed()
}
"#,
    );
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
    _ = net.join_host_port("localhost", "8080")
    _, _ = net.dial("tcp", "127.0.0.1:80")
    _, _ = net.listen("tcp", "127.0.0.1:8080")
}
"#,
    );
}
