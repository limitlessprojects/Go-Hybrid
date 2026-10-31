use hy_check::{Verdict, check_file};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).expect("must parse");
    check_file(&ast)
}

#[test]
fn rejects_closing_recv_only_channel_from_struct_field_fn_call_issue_1358() {
    let r = check(
        "struct holder {\n\
             f fn() int <-chan int\n\
         }\n\
         fn main() {\n\
             ch := make(chan int, 1)\n\
             h := holder{f: fn() (int, <-chan int) { return 1, ch }}\n\
             _, rx := h.f()\n\
             close(rx)\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3009" && d.message.contains("receive-only")),
        "expected HYB-E3009 for closing receive-only channel from struct field fn, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn rejects_sending_to_recv_only_channel_from_struct_field_fn_call_issue_1358() {
    let r = check(
        "struct holder {\n\
             f fn() int <-chan int\n\
         }\n\
         fn main() {\n\
             ch := make(chan int, 1)\n\
             h := holder{f: fn() (int, <-chan int) { return 1, ch }}\n\
             _, rx := h.f()\n\
             rx <- 42\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3008" && d.message.contains("receive-only")),
        "expected HYB-E3008 for sending to receive-only channel from struct field fn, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn rejects_receiving_from_send_only_channel_from_struct_field_fn_call_issue_1358() {
    let r = check(
        "struct holder {\n\
             f fn() int chan<- int\n\
         }\n\
         fn main() {\n\
             ch := make(chan int, 1)\n\
             h := holder{f: fn() (int, chan<- int) { return 1, ch }}\n\
             _, tx := h.f()\n\
             _ = <-tx\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3008" && d.message.contains("send-only")),
        "expected HYB-E3008 for receiving from send-only channel from struct field fn, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn accepts_valid_channel_directions_from_struct_field_fn_call_issue_1358() {
    let r = check(
        "struct holder_rx {\n\
             f fn() int <-chan int\n\
         }\n\
         struct holder_tx {\n\
             f fn() int chan<- int\n\
         }\n\
         fn main() {\n\
             ch := make(chan int, 1)\n\
             hr := holder_rx{f: fn() (int, <-chan int) { return 1, ch }}\n\
             _, rx := hr.f()\n\
             _ = <-rx\n\
             ht := holder_tx{f: fn() (int, chan<- int) { return 2, ch }}\n\
             _, tx := ht.f()\n\
             tx <- 99\n\
             close(tx)\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}
