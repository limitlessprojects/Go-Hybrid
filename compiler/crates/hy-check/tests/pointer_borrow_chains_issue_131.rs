use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

#[test]
fn repro1_ptr_borrow_embedded_in_composite_literal_freezes_pointee() {
    let r = check(
        "struct holder {\n\
           m map[string]int\n\
         }\n\
         fn worker(xs []*holder, done chan<- bool) {\n\
           for i in 0..10 {\n\
             xs[0].m[\"k\"] = i\n\
           }\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           h := holder{m: map[string]int{\"k\": 0}}\n\
           p := &h\n\
           xs := []*holder{p}\n\
           done := make(chan bool, 1)\n\
           task worker(move xs, done)\n\
           for i in 0..10 {\n\
             h.m[\"k\"] = i\n\
           }\n\
           <-done\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3003" && d.tier0_rule.as_deref() == Some("A1b")),
        "expected HYB-E3003 (A1b) on post-move write to pointee `h`, got {:?}",
        r.diagnostics
    );
}

#[test]
fn repro2_ptr_borrow_passed_through_call_freezes_pointee() {
    let r = check(
        "struct holder {\n\
           m map[string]int\n\
         }\n\
         fn id_ptr(p *holder) *holder {\n\
           return move p\n\
         }\n\
         fn worker(p *holder, done chan<- bool) {\n\
           for i in 0..10 {\n\
             p.m[\"k\"] = i\n\
           }\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           h := holder{m: map[string]int{\"k\": 0}}\n\
           p := id_ptr(&h)\n\
           done := make(chan bool, 1)\n\
           task worker(move p, done)\n\
           for i in 0..10 {\n\
             h.m[\"k\"] = i\n\
           }\n\
           <-done\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3003" && d.tier0_rule.as_deref() == Some("A1b")),
        "expected HYB-E3003 (A1b) when `&h` is laundered through `id_ptr(&h)`, got {:?}",
        r.diagnostics
    );
}

#[test]
fn repro3_auto_addressed_ptr_param_on_scalar_struct_freezes_pointee() {
    let r = check(
        "struct stats {\n\
           count int\n\
         }\n\
         fn id_ptr(p *stats) *stats {\n\
           return move p\n\
         }\n\
         fn worker(p *stats, done chan<- bool) {\n\
           for i in 0..10 {\n\
             p.count = i\n\
           }\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           s := stats{count: 0}\n\
           p := id_ptr(s)\n\
           done := make(chan bool, 1)\n\
           task worker(move p, done)\n\
           for i in 0..10 {\n\
             s.count = i\n\
           }\n\
           <-done\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3003" && d.tier0_rule.as_deref() == Some("A1b")),
        "expected HYB-E3003 (A1b) when scalar struct `s` is auto-addressed into `*stats` param, got {:?}",
        r.diagnostics
    );
}

#[test]
fn auto_addressed_ptr_receiver_method_on_scalar_struct_freezes_pointee() {
    let r = check(
        "struct stats {\n\
           count int\n\
         }\n\
         fn (r *stats) ptr() *stats {\n\
           return move r\n\
         }\n\
         fn worker(p *stats, done chan<- bool) {\n\
           p.count = 1\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           s := stats{count: 0}\n\
           p := s.ptr()\n\
           done := make(chan bool, 1)\n\
           task worker(move p, done)\n\
           s.count = 2\n\
           <-done\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3003" && d.tier0_rule.as_deref() == Some("A1b")),
        "expected HYB-E3003 (A1b) when scalar struct `s` is auto-addressed into `*stats` receiver, got {:?}",
        r.diagnostics
    );
}

#[test]
fn repro4_slot_loan_use_after_move_rejected_e3003() {
    let r = check(
        "struct item {\n\
           n int\n\
         }\n\
         fn worker(m map[string]item, start <-chan bool, done chan<- bool) {\n\
           <-start\n\
           for i in 0..10 {\n\
             m[\"k\"] = item{n: i}\n\
           }\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           m := map[string]item{\"k\": item{n: 0}}\n\
           start := make(chan bool, 1)\n\
           done := make(chan bool, 1)\n\
           if true {\n\
             slot := &m[\"k\"]\n\
             slot.n = 1\n\
             task worker(move m, start, done)\n\
             start <- true\n\
             for i in 0..10 {\n\
               slot.n = i\n\
             }\n\
           }\n\
           <-done\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3003" && d.tier0_rule.as_deref() == Some("A1b")),
        "expected HYB-E3003 (A1b) on `slot` use after `move m`, got {:?}",
        r.diagnostics
    );
}

#[test]
fn non_owned_param_borrow_move_rejected_e3004() {
    for src in [
        "struct stats { count int }\n\
         fn worker(p *stats, done chan<- bool) {\n\
           p.count = 1\n\
           done <- true\n\
         }\n\
         fn bad(s stats, done chan<- bool) {\n\
           p := &s\n\
           task worker(move p, done)\n\
         }",
        "struct stats { count int }\n\
         fn id_ptr(p *stats) *stats { return move p }\n\
         fn worker(p *stats, done chan<- bool) {\n\
           p.count = 1\n\
           done <- true\n\
         }\n\
         fn bad(s stats, done chan<- bool) {\n\
           p := id_ptr(s)\n\
           task worker(move p, done)\n\
         }",
        "struct holder { m map[string]int }\n\
         fn worker(xs []*holder, done chan<- bool) {\n\
           xs[0].m[\"k\"] = 1\n\
           done <- true\n\
         }\n\
         fn bad(h holder, done chan<- bool) {\n\
           p := &h\n\
           xs := []*holder{p}\n\
           task worker(move xs, done)\n\
         }",
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.code == "HYB-E3004" && d.tier0_rule.as_deref() == Some("A1b")),
            "expected HYB-E3004 (A1b) for non-owned parameter borrow move, got {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn scalar_return_from_ptr_param_and_dead_slot_loan_accept_positive_control() {
    let r = check(
        "import \"fmt\"\n\
         struct stats {\n\
           count int\n\
         }\n\
         struct item {\n\
           n int\n\
         }\n\
         fn read_count(p *stats) int {\n\
           return p.count\n\
         }\n\
         fn worker_stats(p *stats, done chan<- bool) {\n\
           p.count = 9\n\
           done <- true\n\
         }\n\
         fn worker_map(m map[string]item, done chan<- bool) {\n\
           m[\"k\"] = item{n: 2}\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           s := stats{count: 3}\n\
           n1 := read_count(&s)\n\
           n2 := read_count(s)\n\
           p := &s\n\
           done := make(chan bool, 2)\n\
           task worker_stats(move p, done)\n\
           fmt.println(n1 + n2)\n\
           m := map[string]item{\"k\": item{n: 0}}\n\
           slot := &m[\"k\"]\n\
           slot.n = 1\n\
           task worker_map(move m, done)\n\
           <-done\n\
           <-done\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
}
