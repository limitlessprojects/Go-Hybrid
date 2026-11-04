//! issue_3536_curated_multi_return_counts.rs: regression for #3536.
//!
//! `tuple.rs` enforces T23 (`HYB-E2002`, `#380`) on curated foreign package calls
//! via `foreign::package_result_count`. When functions returning multiple values
//! in Go (`http.get`, `url.parse`, `json.marshal`, `os.pipe`, `net.parse_cidr`, etc.)
//! are bound to fewer variables without `?`, `hyb check` must reject with `HYB-E2002`
//! (GUARANTEES.md Row 13 BUILDABLE / T23).

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

fn rejects_e2002(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E2002"),
        "expected HYB-E2002 for {:?}, got {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(cs.is_empty(), "must accept {:?}, got {:?}", src, cs);
    assert_eq!(r.verdict, Verdict::Accept, "verdict for {:?}", src);
}

#[test]
fn test_url_parse_single_binding_rejected() {
    let src = r#"
import "net/url"

fn main() {
    u := url.parse("https://example.com")
    _ = u
}
"#;
    rejects_e2002(src);
}

#[test]
fn test_url_parse_try_unwrap_accepted() {
    let src = r#"
import "net/url"

fn run() error {
    u := url.parse("https://example.com")?
    _ = u
    return nil
}
"#;
    accepts(src);
}

#[test]
fn test_url_parse_multi_binding_accepted() {
    let src = r#"
import "net/url"

fn main() {
    u, err := url.parse("https://example.com")
    _ = u
    _ = err
}
"#;
    accepts(src);
}

#[test]
fn test_os_pipe_two_bindings_rejected() {
    let src = r#"
import "os"

fn main() {
    r, w := os.pipe()
    _ = r
    _ = w
}
"#;
    rejects_e2002(src);
}

#[test]
fn test_os_pipe_three_bindings_accepted() {
    let src = r#"
import "os"

fn main() {
    r, w, err := os.pipe()
    _ = r
    _ = w
    _ = err
}
"#;
    accepts(src);
}

#[test]
fn test_json_marshal_single_binding_rejected() {
    let src = r#"
import "encoding/json"

fn main() {
    b := json.marshal(1)
    _ = b
}
"#;
    rejects_e2002(src);
}

#[test]
fn test_json_marshal_try_unwrap_accepted() {
    let src = r#"
import "encoding/json"

fn run() error {
    b := json.marshal(1)?
    _ = b
    return nil
}
"#;
    accepts(src);
}

#[test]
fn test_http_get_single_binding_rejected() {
    let src = r#"
import "net/http"

fn main() {
    r := http.get("https://example.com")
    _ = r
}
"#;
    rejects_e2002(src);
}

#[test]
fn test_http_get_multi_binding_accepted() {
    let src = r#"
import "net/http"

fn main() {
    r, err := http.get("https://example.com")
    _ = r
    _ = err
}
"#;
    accepts(src);
}

#[test]
fn test_net_parse_cidr_two_bindings_rejected() {
    let src = r#"
import "net"

fn main() {
    ip, net := net.parse_cidr("192.168.1.1/24")
    _ = ip
    _ = net
}
"#;
    rejects_e2002(src);
}

#[test]
fn test_net_parse_cidr_three_bindings_accepted() {
    let src = r#"
import "net"

fn main() {
    ip, net, err := net.parse_cidr("192.168.1.1/24")
    _ = ip
    _ = net
    _ = err
}
"#;
    accepts(src);
}

#[test]
fn test_os_open_file_single_binding_rejected() {
    let src = r#"
import "os"

fn main() {
    f := os.open_file("test.txt", 0, 0)
    _ = f
}
"#;
    rejects_e2002(src);
}
