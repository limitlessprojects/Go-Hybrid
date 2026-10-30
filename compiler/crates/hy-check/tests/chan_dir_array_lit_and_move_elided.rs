//! Channel direction enforcement (HYB-E3012) across:
//! 1. Typed array composite literals (`[N]T{...}`, `[...]T{...}`) and nested
//!    type-elided composite literals inside array literals.
//! 2. `move`-wrapped type-elided struct and slice/array literals (`move {ch: sc}`,
//!    `move {sc}`) at variable declarations, assignments, call/task arguments,
//!    channel sends, returns, and outer composite literals.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

#[test]
fn typed_array_lit_and_nested_elided_elements_reject_e3012() {
    let r = check(
        "type recv_int = <-chan int\n\
         struct holder {\n\
           ch <-chan int\n\
         }\n\
         fn main() {\n\
           var sc chan<- int = make(chan int, 1)\n\
           a1 := [1]<-chan int{sc}\n\
           a2 := [...]<-chan int{sc}\n\
           a3 := [1]chan int{sc}\n\
           a4 := [1]recv_int{sc}\n\
           a5 := [1]holder{{ch: sc}}\n\
           a6 := [...]holder{{ch: sc}}\n\
           a7 := [1][1]<-chan int{{sc}}\n\
           a8 := [1][]<-chan int{{sc}}\n\
           _ = a1\n\
           _ = a2\n\
           _ = a3\n\
           _ = a4\n\
           _ = a5\n\
           _ = a6\n\
           _ = a7\n\
           _ = a8\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    let e3012_count = r.diagnostics.iter().filter(|d| d.code == "HYB-E3012").count();
    assert_eq!(
        e3012_count, 8,
        "expected 8 HYB-E3012 diagnostics for array literals, got {:?}",
        r.diagnostics
    );
}

#[test]
fn move_wrapped_elided_composite_literals_reject_e3012() {
    let r = check(
        "struct holder {\n\
           ch <-chan int\n\
         }\n\
         fn take_holder(h holder) {\n\
           _ = h\n\
         }\n\
         fn make_bad_holder(sc chan<- int) holder {\n\
           return move {ch: sc}\n\
         }\n\
         fn main() {\n\
           var sc chan<- int = make(chan int, 1)\n\
           var h1 holder = move {ch: sc}\n\
           var h2 holder = holder{ch: make(chan int, 1)}\n\
           h2 = move {ch: sc}\n\
           take_holder(move {ch: sc})\n\
           task take_holder(move {ch: sc})\n\
           ch_h := make(chan holder, 1)\n\
           ch_h <- move {ch: sc}\n\
           s := []holder{move {ch: sc}}\n\
           a := [1]holder{move {ch: sc}}\n\
           m := map[string]holder{\"k\": move {ch: sc}}\n\
           var opt option[holder] = some(move {ch: sc})\n\
           var arr [1]<-chan int = move {sc}\n\
           var sl []<-chan int = move {sc}\n\
           _ = h1\n\
           _ = h2\n\
           _ = s\n\
           _ = a\n\
           _ = m\n\
           _ = opt\n\
           _ = arr\n\
           _ = sl\n\
           _ = make_bad_holder(sc)\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    let e3012_count = r.diagnostics.iter().filter(|d| d.code == "HYB-E3012").count();
    assert_eq!(
        e3012_count, 12,
        "expected 12 HYB-E3012 diagnostics for move-wrapped elided literals, got {:?}",
        r.diagnostics
    );
}

#[test]
fn valid_array_lit_and_nested_elided_chan_directions_accept() {
    let r = check(
        "type recv_int = <-chan int\n\
         struct holder {\n\
           rc <-chan int\n\
           sc chan<- int\n\
         }\n\
         fn take_holder(h holder) {\n\
           _ = h\n\
         }\n\
         fn main() {\n\
           c := make(chan int, 1)\n\
           var rc <-chan int = c\n\
           var sc chan<- int = c\n\
           a1 := [2]<-chan int{c, rc}\n\
           a2 := [...]recv_int{c, rc}\n\
           a3 := [1]holder{{rc: c, sc: sc}}\n\
           a4 := [...]holder{{rc: rc, sc: c}}\n\
           a5 := [1][1]<-chan int{{c}}\n\
           a6 := [1][]<-chan int{{rc}}\n\
           h := holder{rc: rc, sc: c}\n\
           take_holder(move h)\n\
           _ = a1\n\
           _ = a2\n\
           _ = a3\n\
           _ = a4\n\
           _ = a5\n\
           _ = a6\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    assert!(r.diagnostics.is_empty());
}
