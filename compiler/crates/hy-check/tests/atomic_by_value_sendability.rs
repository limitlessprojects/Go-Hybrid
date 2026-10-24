use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("atomic_sendability.hy", src).expect("parse");
    check_file(&ast)
}

#[test]
fn by_value_atomic_at_task_spawn_rejected_with_e3001() {
    let r = check(
        "import \"sync/atomic\"\n\
         fn adder(p *atomic.int64, done chan bool) {\n\
             p.add(1)\n\
             done <- true\n\
         }\n\
         fn by_value_reader(a atomic.int64, out chan int64) {\n\
             out <- a.load()\n\
         }\n\
         fn main() {\n\
             var hits atomic.int64\n\
             done := make(chan bool, 1)\n\
             out := make(chan int64, 1)\n\
             task adder(&hits, done)\n\
             task by_value_reader(hits, out)\n\
             _ = <-out\n\
             <-done\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:#?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "expected HYB-E3001, got {:#?}",
        r.diagnostics
    );
}

#[test]
fn by_value_atomic_at_chan_send_rejected_with_e3002() {
    let r = check(
        "import \"sync/atomic\"\n\
         fn main() {\n\
             var hits atomic.int64\n\
             ch := make(chan atomic.int64, 1)\n\
             ch <- hits\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:#?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3002"),
        "expected HYB-E3002, got {:#?}",
        r.diagnostics
    );
}

#[test]
fn move_struct_containing_by_value_atomic_rejected_with_e3001() {
    let r = check(
        "import \"sync/atomic\"\n\
         struct holder {\n\
             hits atomic.int64\n\
             tag  int\n\
         }\n\
         fn consume(h holder, done chan int) {\n\
             done <- h.tag + int(h.hits.load())\n\
         }\n\
         fn main() {\n\
             h := holder{tag: 1}\n\
             done := make(chan int, 1)\n\
             task consume(move h, done)\n\
             _ = <-done\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:#?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "expected HYB-E3001, got {:#?}",
        r.diagnostics
    );
}

#[test]
fn unmoved_atomic_slice_slot_loan_rejected_at_spawn() {
    let r = check(
        "import \"sync/atomic\"\n\
         fn worker(p *atomic.int64, done chan bool) {\n\
             p.add(1)\n\
             done <- true\n\
         }\n\
         fn main() {\n\
             xs := make([]atomic.int64, 1)\n\
             done := make(chan bool, 1)\n\
             p := &xs[0]\n\
             task worker(p, done)\n\
             <-done\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:#?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "expected HYB-E3001, got {:#?}",
        r.diagnostics
    );
}

#[test]
fn pointer_to_atomic_and_moved_slice_of_atomic_remain_accepted() {
    let r = check(
        "import \"sync/atomic\"\n\
         fn worker_ptr(p *atomic.int64, done chan bool) {\n\
             p.add(1)\n\
             done <- true\n\
         }\n\
         fn worker_slice(xs []atomic.int64, done chan bool) {\n\
             xs[0].add(1)\n\
             done <- true\n\
         }\n\
         fn main() {\n\
             var hits atomic.int64\n\
             xs := make([]atomic.int64, 1)\n\
             d1 := make(chan bool, 1)\n\
             d2 := make(chan bool, 1)\n\
             task worker_ptr(&hits, d1)\n\
             task worker_slice(move xs, d2)\n\
             <-d1\n\
             <-d2\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:#?}", r.diagnostics);
}
