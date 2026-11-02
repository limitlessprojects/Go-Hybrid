use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

#[test]
fn option_unwrap_preserves_reference_shape_across_second_hop_freeze() {
    let r = check(
        "fn worker(m map[int]int, done chan<- bool) {\n\
           m[0] = 1\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           done := make(chan bool, 1)\n\
           m := map[int]int{0: 0}\n\
           opt := some(m)\n\
           m2 := opt.unwrap()\n\
           m3 := m2\n\
           task worker(move m, done)\n\
           m3[0] = 2\n\
           <-done\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 on second-hop alias after option.unwrap(), got {:?}",
        r.diagnostics
    );
}

#[test]
fn nested_option_unwrap_and_none_or_preserve_reference_shape_across_hops() {
    let r_nested = check(
        "fn worker(m map[int]int, done chan<- bool) {\n\
           m[0] = 1\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           done := make(chan bool, 1)\n\
           m := map[int]int{0: 0}\n\
           opt2 := some(some(m))\n\
           opt1 := opt2.unwrap()\n\
           m2 := opt1.unwrap()\n\
           m3 := m2\n\
           task worker(move m, done)\n\
           m3[0] = 2\n\
           <-done\n\
         }",
    );
    assert_eq!(r_nested.verdict, Verdict::Reject, "{:?}", r_nested.diagnostics);
    assert!(
        r_nested.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 on nested option.unwrap() chain, got {:?}",
        r_nested.diagnostics
    );

    let r_or = check(
        "fn worker(m map[int]int, done chan<- bool) {\n\
           m[0] = 1\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           done := make(chan bool, 1)\n\
           m := map[int]int{0: 0}\n\
           opt := none\n\
           m2 := opt.or(m)\n\
           m3 := m2\n\
           task worker(move m, done)\n\
           m3[0] = 2\n\
           <-done\n\
         }",
    );
    assert_eq!(r_or.verdict, Verdict::Reject, "{:?}", r_or.diagnostics);
    assert!(
        r_or.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 on second-hop alias after none.or(m), got {:?}",
        r_or.diagnostics
    );
}

#[test]
fn option_unwrap_of_new_pointer_preserves_pointer_shape_across_hops() {
    let r = check(
        "struct box { x int }\n\
         fn worker(p *box, done chan<- bool) {\n\
           p.x = 1\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           done := make(chan bool, 1)\n\
           p := new(box)\n\
           opt := some(p)\n\
           p2 := opt.unwrap()\n\
           p3 := p2\n\
           task worker(move p, done)\n\
           p3.x = 2\n\
           <-done\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 on second-hop pointer alias after option.unwrap(), got {:?}",
        r.diagnostics
    );
}

#[test]
fn scalar_option_set_and_sync_try_lock_results_are_sendable_at_boundaries() {
    let r = check(
        "import \"sync\"\n\
         struct gate {\n\
           mu sync.mutex\n\
           rw sync.rw_mutex\n\
         }\n\
         fn take_bool(b bool) {}\n\
         fn take_int(n int) {}\n\
         fn main() {\n\
           ch_i := make(chan int, 4)\n\
           ch_b := make(chan bool, 8)\n\
           opt := some(7)\n\
           ch_i <- opt.unwrap()\n\
           ch_i <- opt.or(0)\n\
           ch_b <- opt.is_some()\n\
           ch_b <- opt.is_none()\n\
           var s set[int]\n\
           inserted := s.insert(7)\n\
           has_it := s.contains(7)\n\
           count := s.len()\n\
           ch_b <- inserted\n\
           ch_b <- has_it\n\
           ch_i <- count\n\
           var g gate\n\
           got_m := g.mu.try_lock()\n\
           if got_m { g.mu.unlock() }\n\
           got_r := g.rw.try_rlock()\n\
           if got_r { g.rw.runlock() }\n\
           ch_b <- got_m\n\
           ch_b <- got_r\n\
           task take_bool(opt.is_some())\n\
           task take_int(s.len())\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}
