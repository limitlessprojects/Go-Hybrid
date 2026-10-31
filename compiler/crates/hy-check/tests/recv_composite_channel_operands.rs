//! Receive operands reached through composite channel expressions (#1347).
//!
//! `recv_chan_type_expr` used to resolve only parens / `move` / `try` /
//! nested receives on top of the `ty_of_expr` cache. Composite paths that the
//! cache does not project through — `append(...)[i]`, `<-(<-outer)[i]`,
//! `[ch][0]`, and locals bound from them — came back `None` while
//! `shape_of_expr` still reported `TypeShape::Chan(_)`, so
//! `check_recv_operand` returned `true` without a diagnostic. In the
//! positions where the HYB-E3008 direction walk does not run (type
//! positions: array lengths, ascription types) the send-only misuse was
//! therefore accepted and the emitted Go failed `go build` with
//! `invalid operation: cannot receive from send-only channel`.

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

fn assert_has(r: &hy_check::CheckResult, code: &str) {
    let cs = error_codes(r);
    assert!(
        cs.iter().any(|c| c == code),
        "expected {}, got {:?}",
        code,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

fn assert_accepts(src: &str) {
    let r = check(src);
    let errs = error_codes(&r);
    assert!(
        errs.is_empty(),
        "expected clean accept, got {:?}\nsrc: {}",
        errs,
        src
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn send_only_receive_through_composite_operands_is_rejected() {
    // `append(chs, ch)[0]` in an array-length position: the E3008 direction
    // walk does not visit type positions, so this used to pass with 0
    // diagnostics and emit unbuildable Go.
    assert_has(
        &check(
            r#"package main

fn bad(chs []chan<- int, ch chan<- int) {
    var a [<-append(chs, ch)[0]]int
    _ = a
}
"#,
        ),
        "HYB-E2008",
    );

    // Nested receive: `(<-outer)[0]` where `outer chan []chan<- int`.
    assert_has(
        &check(
            r#"package main

fn bad(outer chan []chan<- int) {
    var a [<-(<-outer)[0]]int
    _ = a
}
"#,
        ),
        "HYB-E2008",
    );

    // NOTE (known remaining sibling, deliberately not pinned here): a local
    // bound from a composite (`sub := append(chs, ch)` then `<-sub[0]`) still
    // passes — the binding carries no inferred `ty`, so there is nothing to
    // project. E3008 catches it in statement positions via its own env walk;
    // a complete fix would reuse that walker for type positions too.

    // Slice projection: `chs[0:1][0]`.
    assert_has(
        &check(
            r#"package main

fn bad(chs []chan<- int) {
    var a [<-chs[0:1][0]]int
    _ = a
}
"#,
        ),
        "HYB-E2008",
    );
}

#[test]
fn composite_operands_on_bidirectional_channels_still_accept() {
    // Same composite paths on a bidirectional channel must not be
    // over-rejected: `append(...)[0]` is a legal `chan int` receive operand.
    assert_accepts(
        r#"package main

fn ok(chs []chan int) int {
    v := <-append(chs, make(chan int, 1))[0]
    return v
}
"#,
    );

    assert_accepts(
        r#"package main

fn ok(outer chan []chan int) int {
    v := <-(<-outer)[0]
    return v
}
"#,
    );

    // And a receive-only element reached the same way stays legal.
    assert_accepts(
        r#"package main

fn ok(chs []<-chan int) int {
    v := <-append(chs, make(chan int, 1))[0]
    return v
}
"#,
    );
}
