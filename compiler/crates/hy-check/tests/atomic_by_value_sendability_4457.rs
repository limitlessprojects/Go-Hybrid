//! Regression (#4457): `atomic.*` is safe across concurrency boundaries ONLY
//! behind `&` (`*atomic.*`, OQ6). Passing an `atomic.*` value by value copies it
//! (`go vet copylocks`) and silently loses every cross-task mutation, so
//! task-spawn (`HYB-E3001`) and channel-send (`HYB-E3002`) must reject by-value
//! atomics while keeping `&hits`, auto-addressed `*atomic.*` params, and `<-`
//! pointer channels accepted.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("inline.hy", src).expect("test source must parse");
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
fn atomic_by_value_at_spawn_and_send_is_rejected() {
    let spawn_by_val = check(
        "import \"sync/atomic\"\n\
         import \"fmt\"\n\
         fn bump(a atomic.int64, done chan<- int) {\n\
             a.add(1)\n\
             done <- 1\n\
         }\n\
         fn main() {\n\
             var hits atomic.int64\n\
             done := make(chan int, 1)\n\
             task bump(hits, done)\n\
             _ = <-done\n\
             fmt.println(\"hits={hits.load()}\")\n\
         }\n",
    );
    assert_eq!(spawn_by_val.verdict, Verdict::Reject, "{:?}", spawn_by_val.diagnostics);
    assert!(
        error_codes(&spawn_by_val).iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001, got {:?}",
        spawn_by_val.diagnostics
    );

    let send_by_val = check(
        "import \"sync/atomic\"\n\
         fn main() {\n\
             var hits atomic.int64\n\
             ch := make(chan atomic.int64, 1)\n\
             ch <- hits\n\
         }\n",
    );
    assert_eq!(send_by_val.verdict, Verdict::Reject, "{:?}", send_by_val.diagnostics);
    assert!(
        error_codes(&send_by_val).iter().any(|c| c == "HYB-E3002"),
        "expected HYB-E3002, got {:?}",
        send_by_val.diagnostics
    );

    let field_by_val = check(
        "import \"sync/atomic\"\n\
         struct counter { hits atomic.int64 }\n\
         fn bump(a atomic.int64, done chan<- int) {\n\
             a.add(1)\n\
             done <- 1\n\
         }\n\
         fn main() {\n\
             var c counter\n\
             done := make(chan int, 1)\n\
             task bump(c.hits, done)\n\
             _ = <-done\n\
         }\n",
    );
    assert_eq!(field_by_val.verdict, Verdict::Reject, "{:?}", field_by_val.diagnostics);
    assert!(
        error_codes(&field_by_val).iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001, got {:?}",
        field_by_val.diagnostics
    );
}

#[test]
fn atomic_by_pointer_and_auto_addressed_stay_accepted() {
    let pointer_route = check(
        "import \"sync/atomic\"\n\
         import \"fmt\"\n\
         fn bump(p *atomic.int64, done chan<- int) {\n\
             p.add(1)\n\
             done <- 1\n\
         }\n\
         fn main() {\n\
             var hits atomic.int64\n\
             done := make(chan int, 1)\n\
             ch := make(chan *atomic.int64, 1)\n\
             task bump(&hits, done)\n\
             ch <- &hits\n\
             _ = <-done\n\
             fmt.println(\"hits={hits.load()}\")\n\
         }\n",
    );
    assert_eq!(pointer_route.verdict, Verdict::Accept, "{:?}", pointer_route.diagnostics);

    let auto_addr_route = check(
        "import \"sync/atomic\"\n\
         fn bump(p *atomic.int64, done chan<- int) {\n\
             p.add(1)\n\
             done <- 1\n\
         }\n\
         fn main() {\n\
             var hits atomic.int64\n\
             done := make(chan int, 1)\n\
             task bump(hits, done)\n\
             _ = <-done\n\
         }\n",
    );
    assert_eq!(auto_addr_route.verdict, Verdict::Accept, "{:?}", auto_addr_route.diagnostics);
}
