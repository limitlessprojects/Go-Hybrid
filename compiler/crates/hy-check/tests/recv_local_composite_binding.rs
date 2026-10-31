//! Locals bound from composite channel expressions — the "known remaining
//! sibling" NOTE deliberately left open by recv_composite_channel_operands.rs
//! (#1347 / #2002).
//!
//! `sub := append(chs, ch)` used to bind no inferred `ty` (the `ty_of_expr`
//! cache does not project through `append`), so a later `<-sub[0]` in a type
//! position — where the E3008 direction walk does not run — had nothing to
//! project and passed with 0 diagnostics while emitting Go that fails
//! `go build` with `invalid operation: cannot receive from send-only
//! channel`. `process_local` now binds the composite-projected type via
//! `composite_type_expr`, so the Index projection resolves the channel and
//! the direction check fires.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn send_only_receive_through_local_composite_is_rejected() {
    // The documented sibling: bind from `append`, then receive through the
    // local in an array-length position. Used to pass with 0 diagnostics.
    let r = check(
        r#"package main

fn bad(chs []chan<- int, ch chan<- int) {
    sub := append(chs, ch)
    var a [<-sub[0]]int
    _ = a
}
"#,
    );
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E2008"),
        "expected HYB-E2008, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn bidirectional_receive_through_local_composite_still_accepts() {
    // Same shape, correctly directed: must stay a clean accept.
    let r = check(
        r#"package main

fn ok(chs []chan int, ch chan int) {
    sub := append(chs, ch)
    var a [<-sub[0]]int
    _ = a
}
"#,
    );
    let cs = error_codes(&r);
    assert!(
        cs.is_empty(),
        "expected clean accept, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn direct_composite_receive_is_unaffected() {
    // The #1347 direct composite (`append(...)` used inline) keeps rejecting.
    let r = check(
        r#"package main

fn bad(chs []chan<- int, ch chan<- int) {
    var a [<-append(chs, ch)[0]]int
    _ = a
}
"#,
    );
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E2008"),
        "expected HYB-E2008, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}
