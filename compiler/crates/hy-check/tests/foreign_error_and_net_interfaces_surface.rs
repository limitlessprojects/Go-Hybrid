use hy_check::{check_file, Verdict};

#[test]
fn phantom_builtin_error_rejected() {
    let src = r#"
struct err_box {
    e error
}

fn main() {
    var b err_box
    b.e.totally_fake_method()
    b.e.message()
    b.e.code()
}
"#;
    let ast = hy_syntax::parse_file("test.hy", src).expect("must parse");
    let res = check_file(&ast);
    assert_eq!(res.verdict, Verdict::Reject);
    let diags: Vec<_> = res
        .diagnostics
        .iter()
        .filter(|d| d.code == "HYB-E1002")
        .collect();
    assert_eq!(diags.len(), 3, "expected 3 HYB-E1002 diags, got: {diags:?}");
    assert!(diags[0].message.contains("`error` has no method `totally_fake_method`"));
    assert!(diags[1].message.contains("`error` has no method `message`"));
    assert!(diags[2].message.contains("`error` has no method `code`"));
}

#[test]
fn phantom_net_addr_and_net_error_rejected() {
    let src = r#"
import "net"

struct net_box {
    a net.addr
    e net.error
}

fn main() {
    var b net_box
    b.a.phantom_addr_call()
    b.a.fake_ip()
    b.e.bogus_net_error()
}
"#;
    let ast = hy_syntax::parse_file("test.hy", src).expect("must parse");
    let res = check_file(&ast);
    assert_eq!(res.verdict, Verdict::Reject);
    let diags: Vec<_> = res
        .diagnostics
        .iter()
        .filter(|d| d.code == "HYB-E1002")
        .collect();
    assert_eq!(diags.len(), 3, "expected 3 HYB-E1002 diags, got: {diags:?}");
    assert!(diags[0].message.contains("`net.addr` has no method `phantom_addr_call`"));
    assert!(diags[1].message.contains("`net.addr` has no method `fake_ip`"));
    assert!(diags[2].message.contains("`net.error` has no method `bogus_net_error`"));
}

#[test]
fn real_error_and_net_interfaces_accept() {
    let src = r#"
import "net"

struct container {
    e error
    a net.addr
    ne net.error
}

fn main() {
    var c container
    _ = c.e.error()
    _ = c.a.network()
    _ = c.a.string()
    _ = c.ne.error()
    _ = c.ne.timeout()
    _ = c.ne.temporary()
}
"#;
    let ast = hy_syntax::parse_file("test.hy", src).expect("must parse");
    let res = check_file(&ast);
    assert_eq!(res.verdict, Verdict::Accept, "expected clean accept, got: {:?}", res.diagnostics);
    assert!(res.diagnostics.is_empty(), "expected 0 diagnostics, got: {:?}", res.diagnostics);
}

#[test]
fn pointer_receivers_enforce_surface() {
    let src = r#"
import "net"

struct ptr_box {
    p *net.addr
    ep *net.error
}

fn main() {
    var b ptr_box
    b.p.invalid_pointer_method()
    b.ep.invalid_error_pointer_call()
}
"#;
    let ast = hy_syntax::parse_file("test.hy", src).expect("must parse");
    let res = check_file(&ast);
    assert_eq!(res.verdict, Verdict::Reject);
    let diags: Vec<_> = res
        .diagnostics
        .iter()
        .filter(|d| d.code == "HYB-E1002")
        .collect();
    assert_eq!(diags.len(), 2, "expected 2 HYB-E1002 diags, got: {diags:?}");
}
