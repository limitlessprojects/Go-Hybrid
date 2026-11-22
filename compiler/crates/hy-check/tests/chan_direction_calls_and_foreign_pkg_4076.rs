//! chan_direction_calls_and_foreign_pkg_4076.rs — regression tests for #4076
//! (HYB-E3008/HYB-E3009 channel direction checks on method/foreign calls and fields).
//!
//! Go spec §Close & §Send:
//! - "It is an error if c is a receive-only channel."
//! - "The channel expression's type must permit send operations."
//!
//! Foreign standard library functions like `time.after(d)` and `time.tick(d)` return
//! `<-chan time.Time` (receive-only). Similarly, `timer.C` and `ticker.C` are receive-only.
//! In `hy-check`, `dir_ty_of_expr` must synthesize channel directions for these call and
//! field expressions so that `close(...)` rejects with `HYB-E3009` and sends reject with
//! `HYB-E3008`.

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
fn close_time_after_rejects_e3009() {
    let r = check(
        "import \"time\"\n\
         fn main() {\n\
           close(time.after(10))\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E3009"),
        "expected HYB-E3009 for close(time.after(10)), got {codes:?}"
    );
}

#[test]
fn send_to_time_after_rejects_e3008() {
    let r = check(
        "import \"time\"\n\
         fn main() {\n\
           time.after(10) <- time.now()\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E3008"),
        "expected HYB-E3008 for send to time.after(10), got {codes:?}"
    );
}

#[test]
fn close_time_tick_rejects_e3009() {
    let r = check(
        "import \"time\"\n\
         fn main() {\n\
           close(time.tick(10))\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E3009"),
        "expected HYB-E3009 for close(time.tick(10)), got {codes:?}"
    );
}

#[test]
fn timer_channel_field_direction_rejects() {
    let r = check(
        "import \"time\"\n\
         fn main() {\n\
           timer := time.new_timer(10)\n\
           close(timer.c)\n\
           timer.c <- time.now()\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E3009"),
        "expected HYB-E3009 for close(timer.c), got {codes:?}"
    );
    assert!(
        codes.iter().any(|c| c == "HYB-E3008"),
        "expected HYB-E3008 for timer.c <- ..., got {codes:?}"
    );
}

#[test]
fn bidir_channel_and_user_send_still_accept() {
    let r = check(
        "fn main() {\n\
           c := make(chan int, 1)\n\
           c <- 42\n\
           close(c)\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    assert!(r.diagnostics.is_empty(), "expected clean accept, got {:?}", r.diagnostics);
}
