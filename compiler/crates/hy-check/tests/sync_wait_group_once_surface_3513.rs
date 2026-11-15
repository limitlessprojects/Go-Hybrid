use hy_check::{Verdict, check_file};
use hy_syntax::parse_file;

#[test]
fn sync_wait_group_and_once_reject_phantom_methods_and_accept_real_ones_3513() {
    for bad in ["struct s { wg sync.wait_group }\nfn main() { var x s\n x.wg.totally_fake() }", "struct s { wg sync.waitgroup }\nfn main() { var x s\n x.wg.totally_fake() }", "struct s { o sync.once }\nfn main() { var x s\n x.o.totally_fake() }"] {
        let r = check_file(&parse_file("t.hy", bad).unwrap());
        assert!(r.verdict == Verdict::Reject && r.diagnostics.iter().any(|d| d.code == "HYB-E1002"), "{bad}: {:?}", r.diagnostics);
    }
    let ok = "struct s { wg sync.wait_group\n o sync.once }\nfn main() { var x s\n x.wg.add(1)\n x.wg.done()\n x.wg.wait()\n x.o.do(fn() {}) }";
    assert_eq!(check_file(&parse_file("t.hy", ok).unwrap()).verdict, Verdict::Accept);
}