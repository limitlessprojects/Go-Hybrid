//! close_range_direction.rs — HYB-E3009: channel direction at `close()` and
//! `for … range` sites (issue #94).
//!
//! Go spec: `close` requires a bidirectional or send-only channel (a
//! `<-chan T` has no send capability); range over a channel requires
//! receive capability (a `chan<- T` cannot be ranged over). Both are
//! invalid Go that the emitter reproduces faithfully, so accepting them
//! breaks the accepted-should-emit-buildable-Go contract (companion to
//! #89/E3008 for send/receive).
//!
//! Controls: close on bidir/send-only and range on bidir/recv-only are
//! valid Go and must stay accepted.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn first_code(r: &hy_check::CheckResult) -> Option<String> {
    r.diagnostics
        .iter()
        .find(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
}

#[test]
fn close_recv_only_rejects_e3009() {
    let r = check(
        "fn main() {\n\
           c := make(chan int, 1)\n\
           var rc <-chan int = c\n\
           close(rc)\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(first_code(&r).as_deref(), Some("HYB-E3009"));
    assert!(r.diagnostics[0].message.contains("receive-only"));
}

#[test]
fn defer_close_recv_only_rejects_e3009() {
    // `defer close(rc)` parses as Defer{call: Call{Ident("close")}} —
    // same direction contract as a direct close statement.
    let r = check(
        "fn main() {\n\
           c := make(chan int, 1)\n\
           var rc <-chan int = c\n\
           defer close(rc)\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(first_code(&r).as_deref(), Some("HYB-E3009"));
}

#[test]
fn close_send_only_ok() {
    // Go allows close on a send-only channel (the send side owns close).
    let r = check(
        "fn main() {\n\
           c := make(chan int, 1)\n\
           var sc chan<- int = c\n\
           close(sc)\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    assert!(r.diagnostics.is_empty());
}

#[test]
fn close_bidir_ok() {
    let r = check(
        "fn main() {\n\
           c := make(chan int, 1)\n\
           close(c)\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    assert!(r.diagnostics.is_empty());
}

#[test]
fn range_send_only_rejects_e3009() {
    let r = check(
        "fn main() {\n\
           c := make(chan int, 1)\n\
           var sc chan<- int = c\n\
           for v in sc {\n\
             fmt.println(v)\n\
           }\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(first_code(&r).as_deref(), Some("HYB-E3009"));
    assert!(r.diagnostics[0].message.contains("send-only"));
}

#[test]
fn range_recv_only_ok() {
    // Go allows ranging over a receive-only channel.
    let r = check(
        "fn main() {\n\
           c := make(chan int, 1)\n\
           var rc <-chan int = c\n\
           for v in rc {\n\
             fmt.println(v)\n\
           }\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    assert!(r.diagnostics.is_empty());
}

#[test]
fn range_bidir_ok() {
    let r = check(
        "fn main() {\n\
           c := make(chan int, 1)\n\
           for v in c {\n\
             fmt.println(v)\n\
           }\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    assert!(r.diagnostics.is_empty());
}

#[test]
fn close_unresolvable_name_no_false_positive() {
    // A close on a name the checker cannot resolve does not false-positive:
    // direction does not resolve → no E3009 (the O4.9/fail-closed paths own
    // unresolvable values at send/spawn sites, not close).
    let r = check(
        "fn main() {\n\
           close(mystery_chan)\n\
         }",
    );
    assert!(
        !r.diagnostics.iter().any(|d| d.code == "HYB-E3009"),
        "no E3009 on unresolvable close target: {:?}",
        r.diagnostics
    );
}
