//! Soundness tests for net package member call arity and non-callable symbols (HYB-E1002, Row 13 BUILDABLE).
//! Guarantees that invocations of net package functions validate exact parameter arity,
//! and that package error variables, constants, and types are rejected fail-closed with HYB-E1002.

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn test_net_non_callable_variables_constants_types_rejected() {
    let src = r#"
package main

import "net"

fn main() {
    _ = net.err_closed()
    _ = net.err_write_to_connected()
    _ = net.ipv4len()
    _ = net.ipv6len()
    _ = net.conn()
    _ = net.listener()
    _ = net.packet_conn()
    _ = net.ip()
    _ = net.tcp_addr()
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject, "expected reject, got {:?}", res.diagnostics);
    let all = codes(&res);
    assert_eq!(all.len(), 9, "expected 9 diagnostics, got: {:?}", all);
    for c in &all {
        assert_eq!(c, "HYB-E1002");
    }
}

#[test]
fn test_net_dial_and_listen_arity_rejected() {
    let bad_d0 = r#"
package main

import "net"

fn main() {
    c, err := net.dial()
    _ = c
    _ = err
}
"#;
    let res_d0 = check(bad_d0);
    assert_eq!(res_d0.verdict, Verdict::Reject);
    assert!(codes(&res_d0).contains(&"HYB-E1002".to_string()));

    let bad_d1 = r#"
package main

import "net"

fn test_it(network string) {
    c, err := net.dial(network)
    _ = c
    _ = err
}
"#;
    let res_d1 = check(bad_d1);
    assert_eq!(res_d1.verdict, Verdict::Reject);
    assert!(codes(&res_d1).contains(&"HYB-E1002".to_string()));

    let bad_dt2 = r#"
package main

import "net"

fn test_it(network string, addr string) {
    c, err := net.dial_timeout(network, addr)
    _ = c
    _ = err
}
"#;
    let res_dt2 = check(bad_dt2);
    assert_eq!(res_dt2.verdict, Verdict::Reject);
    assert!(codes(&res_dt2).contains(&"HYB-E1002".to_string()));

    let bad_l1 = r#"
package main

import "net"

fn test_it(network string) {
    l, err := net.listen(network)
    _ = l
    _ = err
}
"#;
    let res_l1 = check(bad_l1);
    assert_eq!(res_l1.verdict, Verdict::Reject);
    assert!(codes(&res_l1).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_net_parse_ip_and_lookup_arity_rejected() {
    let bad_p0 = r#"
package main

import "net"

fn main() {
    _ = net.parse_ip()
}
"#;
    let res_p0 = check(bad_p0);
    assert_eq!(res_p0.verdict, Verdict::Reject);
    assert!(codes(&res_p0).contains(&"HYB-E1002".to_string()));

    let bad_p2 = r#"
package main

import "net"

fn test_it(s string) {
    _ = net.parse_ip(s, 1)
}
"#;
    let res_p2 = check(bad_p2);
    assert_eq!(res_p2.verdict, Verdict::Reject);
    assert!(codes(&res_p2).contains(&"HYB-E1002".to_string()));

    let bad_lh0 = r#"
package main

import "net"

fn main() {
    addrs, err := net.lookup_host()
    _ = addrs
    _ = err
}
"#;
    let res_lh0 = check(bad_lh0);
    assert_eq!(res_lh0.verdict, Verdict::Reject);
    assert!(codes(&res_lh0).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_net_pipe_and_ipv4_arity_rejected() {
    let bad_pipe1 = r#"
package main

import "net"

fn main() {
    c1, c2 := net.pipe(1)
    _ = c1
    _ = c2
}
"#;
    let res_p1 = check(bad_pipe1);
    assert_eq!(res_p1.verdict, Verdict::Reject);
    assert!(codes(&res_p1).contains(&"HYB-E1002".to_string()));

    let bad_ipv4_2 = r#"
package main

import "net"

fn main() {
    _ = net.ipv4(127, 0)
}
"#;
    let res_ip2 = check(bad_ipv4_2);
    assert_eq!(res_ip2.verdict, Verdict::Reject);
    assert!(codes(&res_ip2).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_net_valid_constructors_and_references_accept() {
    let good = r#"
package main

import "net"

fn run(network string, addr string, timeout any, s string) {
    c, err := net.dial(network, addr)
    _ = c
    _ = err
    ct, errt := net.dial_timeout(network, addr, timeout)
    _ = ct
    _ = errt
    l, errl := net.listen(network, addr)
    _ = l
    _ = errl
    ip := net.parse_ip(s)
    _ = ip
    c1, c2 := net.pipe()
    _ = c1
    _ = c2
    ip4 := net.ipv4(127, 0, 0, 1)
    _ = ip4

    ec := net.err_closed
    _ = ec
    v4len := net.ipv4len
    _ = v4len
}

fn main() {
    fmt.println(1)
}
"#;
    let res = check(good);
    assert_eq!(res.verdict, Verdict::Accept, "expected accept, got diagnostics: {:?}", res.diagnostics);
    assert!(res.diagnostics.is_empty(), "expected 0 diagnostics, got: {:?}", res.diagnostics);
}
