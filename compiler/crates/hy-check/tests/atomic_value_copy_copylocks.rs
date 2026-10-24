use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("atomic_copylocks.hy", src).expect("parse");
    check_file(&ast)
}

#[test]
fn overwriting_shared_atomic_by_value_rejected_with_e1002() {
    let r = check(
        "import \"sync/atomic\"\n\
         fn worker(p *atomic.int64, done chan bool) {\n\
             p.add(1)\n\
             done <- true\n\
         }\n\
         fn main() {\n\
             var hits atomic.int64\n\
             var zero atomic.int64\n\
             done := make(chan bool, 1)\n\
             task worker(&hits, done)\n\
             hits = zero\n\
             <-done\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:#?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "expected HYB-E1002, got {:#?}",
        r.diagnostics
    );
}

#[test]
fn copying_atomic_into_local_return_or_value_param_rejected_with_e1002() {
    for src in [
        "import \"sync/atomic\"\n\
         fn main() {\n\
             var hits atomic.int64\n\
             other := hits\n\
             _ = other.load()\n\
         }\n",
        "import \"sync/atomic\"\n\
         fn make_counter() atomic.int64 {\n\
             var hits atomic.int64\n\
             return hits\n\
         }\n\
         fn main() {\n\
             _ = make_counter()\n\
         }\n",
        "import \"sync/atomic\"\n\
         fn read_val(a atomic.int64) int64 {\n\
             return a.load()\n\
         }\n\
         fn main() {\n\
             var hits atomic.int64\n\
             _ = read_val(hits)\n\
         }\n",
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "{src}\n{:#?}", r.diagnostics);
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
            "expected HYB-E1002 for:\n{src}\ngot {:#?}",
            r.diagnostics
        );
    }
}

#[test]
fn auto_addressed_pointer_param_and_fresh_struct_init_remain_accepted() {
    let r = check(
        "import \"fmt\"\n\
         import \"sync/atomic\"\n\
         struct holder {\n\
             hits atomic.int64\n\
             tag  int\n\
         }\n\
         fn bump(p *atomic.int64) {\n\
             p.add(1)\n\
         }\n\
         fn main() {\n\
             var hits atomic.int64\n\
             h := holder{tag: 7}\n\
             bump(&hits)\n\
             bump(hits)\n\
             h.hits.add(2)\n\
             fmt.println(hits.load(), h.hits.load(), h.tag)\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:#?}", r.diagnostics);
}
