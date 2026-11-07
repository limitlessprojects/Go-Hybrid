//! Tests for curated foreign method surfaces of `net.conn`, `net.listener`, `net.tcp_conn`,
//! `net.tcp_listener`, `net.udp_conn`, `net.unix_conn`, and `net.unix_listener` (HYB-E1002, Row 13 BUILDABLE).
//! Guarantees that calling phantom/unsupported methods on these net handles rejects fail-closed with HYB-E1002,
//! while all real Go standard library methods accept cleanly.

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn phantom_net_conn_methods_are_rejected() {
    let bad = check(
        "struct holder {\n\
             c net.conn\n\
         }\n\
         fn main() {\n\
             var h holder\n\
             h.c.totally_fake_method()\n\
             fmt.println(1)\n\
         }\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject, "expected reject, got {:?}", bad.diagnostics);
    assert!(
        codes(&bad).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        codes(&bad)
    );
}

#[test]
fn pointer_net_conn_phantom_method_is_rejected() {
    let bad = check(
        "fn check_conn(c *net.conn) {\n\
             c.unknown_network_method()\n\
             fmt.println(1)\n\
         }\n\
         fn main() {}\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject, "expected reject, got {:?}", bad.diagnostics);
    assert!(
        codes(&bad).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        codes(&bad)
    );
}

#[test]
fn phantom_net_listener_methods_are_rejected() {
    let bad = check(
        "struct holder {\n\
             l net.listener\n\
         }\n\
         fn main() {\n\
             var h holder\n\
             h.l.phantom_listener_call()\n\
             fmt.println(1)\n\
         }\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject, "expected reject, got {:?}", bad.diagnostics);
    assert!(
        codes(&bad).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        codes(&bad)
    );
}

#[test]
fn phantom_tcp_conn_and_listener_methods_are_rejected() {
    let bad_tcp = check(
        "struct holder {\n\
             tc net.tcp_conn\n\
             tl net.tcp_listener\n\
         }\n\
         fn main() {\n\
             var h holder\n\
             h.tc.totally_fake()\n\
             h.tl.totally_fake()\n\
             fmt.println(1)\n\
         }\n",
    );
    assert_eq!(bad_tcp.verdict, Verdict::Reject, "expected reject, got {:?}", bad_tcp.diagnostics);
    assert!(
        codes(&bad_tcp).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        codes(&bad_tcp)
    );
}

#[test]
fn net_package_functions_called_as_methods_are_rejected() {
    for pkg_fn in ["dial", "listen", "parse_ip", "resolve_tcp_addr"] {
        let src = format!(
            "struct holder {{\n\
                 c net.conn\n\
             }}\n\
             fn main() {{\n\
                 var h holder\n\
                 h.c.{pkg_fn}()\n\
                 fmt.println(1)\n\
             }}\n"
        );
        let r = check(&src);
        assert_eq!(r.verdict, Verdict::Reject, "expected reject for {pkg_fn}, got {:?}", r.diagnostics);
        assert!(
            codes(&r).iter().any(|c| c == "HYB-E1002"),
            "expected HYB-E1002 for {pkg_fn}, got {:?}",
            codes(&r)
        );
    }
}

#[test]
fn real_net_conn_and_listener_methods_accept() {
    let good = check(
        "struct holder {\n\
             c net.conn\n\
             l net.listener\n\
             tc net.tcp_conn\n\
             tl net.tcp_listener\n\
         }\n\
         fn main() {\n\
             var h holder\n\
             _ = h.c.local_addr()\n\
             _ = h.c.remote_addr()\n\
             _ = h.c.close()\n\
             _ = h.l.addr()\n\
             _ = h.l.close()\n\
             _ = h.tc.close_read()\n\
             _ = h.tc.close_write()\n\
             _ = h.tc.local_addr()\n\
             _ = h.tl.addr()\n\
             _ = h.tl.close()\n\
             fmt.println(1)\n\
         }\n",
    );
    assert_eq!(good.verdict, Verdict::Accept, "expected accept, got {:?}", good.diagnostics);
}
