use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn paren_task_arg_unmoved_and_addr_rejected_e3002() {
    let src = "fn w(tx hy.send_cap[int]) {}\nfn main() {\ntx, rx := hy.pipe[int]()\ntask w((tx))\ntask w((&tx))\ntx.close()\nrx.drain()\n}\n";
    let r = check_file(&parse_file("repro_4868_a.hy", src).unwrap());
    assert_eq!(r.verdict, Verdict::Reject);
    assert_eq!(r.diagnostics.iter().filter(|d| d.code == "HYB-E3002").count(), 2);
}
