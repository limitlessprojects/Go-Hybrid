use hy_check::{check_file, CheckResult, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    check_file(&parse_file("inline.hy", src).expect("parse"))
}

fn has_code(r: &CheckResult, code: &str) -> bool {
    r.diagnostics.iter().any(|d| d.code == code)
}

#[test]
fn move_struct_or_container_with_unproven_atomic_or_foreign_leaf_rejects_e3005() {
    for src in [
        "import \"sync/atomic\"\nstruct atomic_box {\nv atomic.value\np atomic.pointer[int]\n}\nfn worker(b atomic_box) {}\nfn main() {\nvar b atomic_box\ntask worker(move b)\n}",
        "import \"sync/atomic\"\nstruct ptr_box { p atomic.pointer[int] }\nfn main() {\nvar pb ptr_box\nch := make(chan ptr_box, 1)\nch <- move pb\n}",
        "import \"sync/atomic\"\nfn worker(xs []atomic.value) {}\nfn main() {\nvar xs []atomic.value\ntask worker(move xs)\n}",
    ] {
        let r = check(src);
        assert!(r.verdict == Verdict::Reject && has_code(&r, "HYB-E3005"), "{:?}", r.diagnostics);
    }
    let r_foreign = check("import \"net/url\"\nstruct query_box { q url.values }\nfn main() {\nvar qb query_box\nch := make(chan query_box, 1)\nch <- move qb\n}");
    assert!(r_foreign.verdict == Verdict::Reject && has_code(&r_foreign, "HYB-E3002"), "{:?}", r_foreign.diagnostics);
    let r_ok = check("import \"fmt\"\nstruct box[t] { v t }\nfn worker(b box[int], done chan<- bool) {\nfmt.println(b.v)\ndone <- true\n}\nfn main() {\nb := box[int]{v: 42}\ndone := make(chan bool, 1)\ntask worker(move b, done)\n<-done\n}");
    assert_eq!(r_ok.verdict, Verdict::Accept, "{:?}", r_ok.diagnostics);
}

#[test]
fn move_in_boundary_subexpression_enforces_sendability_and_managed_lint() {
    let r1 = check("struct holder {\nf fn()\nn int\n}\nfn worker(n int) {}\nfn main() {\nx := 1\nf := fn() { x = 2 }\ntask worker(holder{f: move f, n: 1}.n)\n}");
    assert!(r1.verdict == Verdict::Reject && has_code(&r1, "HYB-E3001"), "{:?}", r1.diagnostics);
    for src in [
        "fn worker(n int) {}\nfn main() {\nmanaged(\"seed map\", \"mutex\") {\nm := map[string]int{\"k\": 7}\ntask worker((move m)[\"k\"])\n}\n}",
        "fn worker(fs []fn()) {}\nfn main() {\nx := 1\nf := fn() { x = 2 }\nmanaged(\"actor callbacks\", \"actor\") {\ntask worker(move []fn(){f})\n}\n}",
    ] {
        let r = check(src);
        assert!(has_code(&r, "HYB-E8001") && r.managed.sites_via_managed == 1, "{:?}", r.diagnostics);
    }
}

#[test]
fn chan_dir_preserved_across_move_multi_return_and_new_allocations() {
    let r1 = check("fn pair_send_only() (chan<- int, int) {\nch := make(chan int, 1)\nreturn ch, 1\n}\nfn take_recv_only(ch <-chan int, n int) {}\nfn bad_return() (<-chan int, int) {\nreturn move pair_send_only()\n}\nfn main() {\nch, n := move pair_send_only()\nv := <-ch\nfor x in ch {}\nvar r <-chan int\nvar m int\nr, m = move pair_send_only()\ntake_recv_only(move pair_send_only())\n}");
    assert!(r1.verdict == Verdict::Reject && has_code(&r1, "HYB-E3008") && has_code(&r1, "HYB-E3009") && r1.diagnostics.iter().filter(|d| d.code == "HYB-E3012").count() == 3, "{:?}", r1.diagnostics);
    let r2 = check("type recv_arr = [1]<-chan int\nstruct holder {\nch <-chan int\nout chan<- int\n}\nstruct box[t] { v t }\nfn main() {\narr_ptr := new(recv_arr)\narr_ptr[0] <- 1\nclose(arr_ptr[0])\nh_ptr := new(holder)\nh_ptr.ch <- 2\nclose(h_ptr.ch)\nvar s chan<- int = h_ptr.ch\nv := <-h_ptr.out\nb_ptr := new(box[<-chan int])\nb_ptr.v <- 3\n}");
    assert!(r2.verdict == Verdict::Reject && r2.diagnostics.iter().filter(|d| d.code == "HYB-E3008").count() == 4 && r2.diagnostics.iter().filter(|d| d.code == "HYB-E3009").count() == 2 && r2.diagnostics.iter().filter(|d| d.code == "HYB-E3012").count() == 1, "{:?}", r2.diagnostics);
}
