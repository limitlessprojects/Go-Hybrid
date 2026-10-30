//! make_directional_channel_soundness.rs — tests for HYB-E2005 rejection
//! of directional channel types in `make`.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn make_recv_only_channel_literal_rejects_e2005() {
    let src = "\
fn main() {
    _ = make(<-chan int)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E2005".to_string()));
    assert!(r.diagnostics.iter().any(|d| d.message.contains("bidirectional")));
}

#[test]
fn make_send_only_channel_literal_rejects_e2005() {
    let src = "\
fn main() {
    _ = make(chan<- int, 5)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E2005".to_string()));
    assert!(r.diagnostics.iter().any(|d| d.message.contains("bidirectional")));
}

#[test]
fn make_recv_only_channel_alias_rejects_e2005() {
    let src = "\
type recv_chan = <-chan int

fn main() {
    _ = make(recv_chan)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E2005".to_string()));
    assert!(r.diagnostics.iter().any(|d| d.message.contains("bidirectional")));
}

#[test]
fn make_send_only_channel_alias_rejects_e2005() {
    let src = "\
type send_chan = chan<- int

fn main() {
    _ = make(send_chan, 10)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E2005".to_string()));
    assert!(r.diagnostics.iter().any(|d| d.message.contains("bidirectional")));
}

#[test]
fn make_bidirectional_channel_accepts() {
    let src = "\
type bidirectional_chan = chan int

fn main() {
    _ = make(chan int)
    _ = make(chan string, 10)
    _ = make(bidirectional_chan, 2)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected accept, got diagnostics: {:?}", r.diagnostics);
}
