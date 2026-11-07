//! Issue #4046: the curated `net` connection and listener handles need method
//! surfaces.
//!
//! `foreign_method_surface` modelled the `net` *address* types (`net.ip`,
//! `net.tcp_addr`, `net.udp_addr`, `net.unix_addr`, ...) but had no arm for the
//! connection or listener handles, so a phantom method on one of them fell
//! through to `ForeignOpaque`:
//!
//! ```hybrid
//! import "net"
//! fn bad(c net.conn) { _ = c.totally_fake_method() }
//! ```
//!
//! `hyb check` printed `OK (0 diagnostics)`; `go build` said
//! `c.totally_fake_method undefined (type net.Conn has no field or method
//! totally_fake_method)`. A Row 13 BUILDABLE break.
//!
//! What this file pins:
//!   1. a phantom method is rejected on **each** of the seven receivers the
//!      report names, not just the one in the repro;
//!   2. a real method from `go doc` is accepted on each of them, so the fix
//!      pinned the true surface rather than blanket-rejecting;
//!   3. an interface method (`net.conn.write`) and a concrete-only method
//!      (`net.tcp_conn.set_no_delay`) both resolve — the two families are
//!      deliberately different and a copy-paste of one into the other would
//!      break valid code.

use hy_check::{check_file, CheckResult, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

const PRELUDE: &str = "package main\n\nimport \"net\"\n\n";

/// (receiver spelling, a real method on it)
const RECEIVERS: &[(&str, &str)] = &[
    ("net.conn", "write"),
    ("net.listener", "accept"),
    ("net.tcp_conn", "set_no_delay"),
    ("net.tcp_listener", "accept_tcp"),
    ("net.udp_conn", "read_from_udp"),
    ("net.unix_conn", "close_write"),
    ("net.unix_listener", "accept_unix"),
];

#[test]
fn phantom_method_rejected_on_every_receiver() {
    for (recv, _) in RECEIVERS {
        let src = format!(
            "{PRELUDE}fn bad(c {recv}) {{\n    _ = c.totally_fake_method()\n}}\n\nfn main() {{}}\n"
        );
        let r = check(&src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "{recv} must reject a phantom method, got {:?}",
            r.diagnostics
        );
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.code == "HYB-E1002" && d.message.contains("totally_fake_method")),
            "{recv}: expected E1002 naming the method, got: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn real_method_accepted_on_every_receiver() {
    // If any arm in the new table is wrong (a typo, a name that does not exist
    // in `go doc net.<Type>`), this fails — which is the failure mode that
    // matters, because an over-tight surface breaks valid programs.
    for (recv, method) in RECEIVERS {
        // Give the call the arity the real method needs so the pin is about the
        // NAME resolving, not about argument checking.
        let call = match *method {
            "write" => "c.write([]byte{})".to_string(),
            "accept" | "accept_tcp" | "accept_unix" => format!("_ = c.{method}()"),
            "read_from_udp" => "c.read_from_udp([]byte{})".to_string(),
            m => format!("c.{m}()"),
        };
        let src = format!("{PRELUDE}fn good(c {recv}) {{\n    {call}\n}}\n\nfn main() {{}}\n");
        let r = check(&src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "{recv}.{method} must be accepted, got: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn interface_and_concrete_surfaces_stay_distinct() {
    // `set_no_delay` exists on *net.TCPConn only, not on net.Conn. A table
    // copy-paste that flattened the two would silently start accepting it on
    // the interface, so pin the negative side too.
    let on_conn = check(&format!(
        "{PRELUDE}fn bad(c net.conn) {{\n    c.set_no_delay(true)\n}}\n\nfn main() {{}}\n"
    ));
    assert_eq!(
        on_conn.verdict,
        Verdict::Reject,
        "net.conn must not gain net.TCPConn-only methods, got: {:?}",
        on_conn.diagnostics
    );

    let on_tcp = check(&format!(
        "{PRELUDE}fn good(c net.tcp_conn) {{\n    c.set_no_delay(true)\n}}\n\nfn main() {{}}\n"
    ));
    assert_eq!(
        on_tcp.verdict,
        Verdict::Accept,
        "net.tcp_conn must accept set_no_delay, got: {:?}",
        on_tcp.diagnostics
    );
}
