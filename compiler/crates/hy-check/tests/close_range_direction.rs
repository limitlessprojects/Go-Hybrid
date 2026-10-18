//! close_range_direction.rs — HYB-E3009: channel direction at `close()` and
//! `for … in` sites (issue #94).
//!
//! Go spec: `close` requires a bidirectional or send-only channel (a
//! `<-chan T` has no send capability); range over a channel requires
//! receive capability (a `chan<- T` cannot be ranged over). Both are
//! invalid Go that the emitter reproduces faithfully, so accepting them
//! breaks the accepted-should-emit-buildable-Go contract.
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
        "import \"fmt\"\n\
         fn main() {\n\
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
    let r = check(
        "import \"fmt\"\n\
         fn main() {\n\
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
        "import \"fmt\"\n\
         fn main() {\n\
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
fn close_inside_closure_and_struct_field_rejects_e3009() {
    let r = check(
        "struct reader {\n\
           rc <-chan int\n\
         }\n\
         fn main() {\n\
           c := make(chan int, 1)\n\
           r := reader{rc: c}\n\
           f := fn() {\n\
             close(r.rc)\n\
           }\n\
           f()\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(first_code(&r).as_deref(), Some("HYB-E3009"));
}

#[test]
fn range_over_fn_returning_send_only_rejects_e3009() {
    let r = check(
        "import \"fmt\"\n\
         fn sink(c chan int) chan<- int {\n\
           return c\n\
         }\n\
         fn main() {\n\
           c := make(chan int, 1)\n\
           for v in sink(c) {\n\
             fmt.println(v)\n\
           }\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(first_code(&r).as_deref(), Some("HYB-E3009"));
}

#[test]
fn close_unresolvable_name_no_false_positive() {
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

#[test]
fn send_to_recv_only_param_and_closure_rejects_e3008_issue_89() {
    let r = check(
        "fn worker(ch <-chan int) {
           ch <- 1
         }
         fn main() {
           c := make(chan int, 1)
           var rc <-chan int = c
           body := fn() {
             rc <- 1
           }
           body()
           worker(c)
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    let codes: Vec<&str> = r.diagnostics.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(codes, vec!["HYB-E3008", "HYB-E3008"]);
}

#[test]
fn recv_from_send_only_param_and_select_arms_reject_e3008_issue_89() {
    let r = check(
        "import \"fmt\"
         fn worker(ch chan<- int) {
           <-ch
         }
         fn main() {
           c := make(chan int, 1)
           var sc chan<- int = c
           var rc <-chan int = c
           v := <-sc
           fmt.println(v)
           select {
           x := <-sc:
             fmt.println(x)
           rc <- 1:
             fmt.println(1)
           }
           worker(c)
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    let codes: Vec<&str> = r.diagnostics.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(
        codes,
        vec!["HYB-E3008", "HYB-E3008", "HYB-E3008", "HYB-E3008"]
    );
}
