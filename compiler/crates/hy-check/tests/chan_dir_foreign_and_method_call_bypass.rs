//! chan_dir_foreign_and_method_call_bypass.rs — HYB-E3008 / HYB-E3009:
//! channel direction checks in built-in close() and Stmt::Send for foreign
//! package calls, foreign handle fields, and method returns (fixes #4076).
//!
//! Go spec §Close & §Send:
//! "It is an error if c is a receive-only channel."
//! "The send statement sends a value on a channel. The channel expression's
//!  type must permit send operations."
//!
//! Curated foreign calls returning receive-only channels (`time.after(d)`,
//! `time.tick(d)`), curated foreign fields (`time.Ticker.C`), and struct
//! method calls returning receive-only channels must reject close() with
//! HYB-E3009 and sends with HYB-E3008.

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
fn foreign_time_after_close_rejects_e3009() {
    let r = check(
        "import \"time\"\n\
         fn main() {\n\
             close(time.after(10))\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(codes.contains(&"HYB-E3009".to_string()), "expected HYB-E3009, got {:?}", codes);
}

#[test]
fn foreign_time_after_send_rejects_e3008() {
    let r = check(
        "import \"time\"\n\
         fn main() {\n\
             time.after(10) <- time.now()\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(codes.contains(&"HYB-E3008".to_string()), "expected HYB-E3008, got {:?}", codes);
}

#[test]
fn method_returning_recv_chan_close_and_send_reject() {
    let r = check(
        "struct Receiver {\n\
             rx <-chan int\n\
         }\n\
         fn (r Receiver) get_rx() <-chan int {\n\
             return r.rx\n\
         }\n\
         fn main() {\n\
             var r Receiver\n\
             close(r.get_rx())\n\
             r.get_rx() <- 42\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(codes.contains(&"HYB-E3009".to_string()), "expected HYB-E3009, got {:?}", codes);
    assert!(codes.contains(&"HYB-E3008".to_string()), "expected HYB-E3008, got {:?}", codes);
}

#[test]
fn chan_dir_valid_controls_stay_accepted() {
    let r = check(
        "fn main() {\n\
             c := make(chan int, 1)\n\
             c <- 1\n\
             close(c)\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}
