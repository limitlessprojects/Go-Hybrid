//! Regression tests for #3716: `recv_chan_type_expr` must project `Expr::Field`
//! through composite channel bases (`Receive`, `Slice`, `append`, and locals
//! bound from them).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).expect("must parse");
    check_file(&ast)
}

fn assert_has(src: &str, code: &str) {
    let r = check(src);
    let cs: Vec<_> = r.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.code.as_str()).collect();
    assert!(cs.contains(&code), "expected {code}, got {cs:?} for:\n{src}");
    assert_eq!(r.verdict, Verdict::Reject);
}

fn assert_accepts(src: &str) {
    let r = check(src);
    let cs: Vec<_> = r.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.code.as_str()).collect();
    assert!(cs.is_empty(), "expected accept, got {cs:?} for:\n{src}");
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn send_only_channel_field_on_composite_base_is_rejected() {
    assert_has(
        "package main\nstruct holder { ch chan<- int }\nfn bad(outer chan holder) {\n    var a [<-(<-outer).ch]int\n    _ = a\n}\n",
        "HYB-E2008",
    );
    assert_has(
        "package main\nstruct holder { ch chan<- int }\nfn bad(boxes []holder, b holder) {\n    var a [<-append(boxes, b)[0].ch]int\n    _ = a\n}\n",
        "HYB-E2008",
    );
    assert_has(
        "package main\nstruct holder { ch chan<- int }\nfn bad(outer chan holder) {\n    sub := (<-outer).ch\n    var a [<-sub]int\n    _ = a\n}\n",
        "HYB-E2008",
    );
    assert_has(
        "package main\nstruct holder { ch chan<- int }\nfn bad(boxes []holder) {\n    var a [<-boxes[0:1][0].ch]int\n    _ = a\n}\n",
        "HYB-E2008",
    );
}

#[test]
fn readable_channel_field_on_composite_base_still_accepts() {
    assert_accepts(
        "package main\nstruct holder { ch chan int }\nfn ok(outer chan holder) int {\n    return <-(<-outer).ch\n}\n",
    );
    assert_accepts(
        "package main\nstruct holder { ch <-chan int }\nfn ok(boxes []holder) int {\n    return <-boxes[0:1][0].ch\n}\n",
    );
}
