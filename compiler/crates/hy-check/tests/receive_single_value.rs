//! receive_single_value.rs — regression tests for #1860: a channel receive
//! operand must be exactly one value.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn multi_value_receive_operand_is_e1002() {
    let r = check(
        "fn two_chans() (chan int, chan int) {\n    return make(chan int, 1), make(chan int, 1)\n}\n\nfn main() {\n    v := <-two_chans()\n    _ = v\n}\n",
    );
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for `<-two_chans()`, got {cs:?}"
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn void_receive_operand_is_e1002() {
    let r = check(
        "fn nothing() {\n}\n\nfn main() {\n    v := <-nothing()\n    _ = v\n}\n",
    );
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for a void receive operand, got {cs:?}"
    );
}

#[test]
fn single_channel_receive_accepts() {
    let r = check(
        "fn main() {\n    c := make(chan int, 1)\n    c <- 7\n    v := <-c\n    _ = v\n}\n",
    );
    let cs = codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E1002"),
        "an ordinary receive must stay accepted, got {cs:?}"
    );
}
