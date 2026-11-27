use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn send_or_close_after_close_rejected_e4004() {
    let src = "fn main() {\ntx, rx := hy.pipe[int]()\ntx.close()\ntx.send(1)\ntx <- 2\ntx.close()\nrx.drain()\n}\n";
    let r = check_file(&parse_file("repro_4868_b.hy", src).unwrap());
    assert_eq!(r.verdict, Verdict::Reject);
    assert_eq!(r.diagnostics.iter().filter(|d| d.code == "HYB-E4004").count(), 3);
}
