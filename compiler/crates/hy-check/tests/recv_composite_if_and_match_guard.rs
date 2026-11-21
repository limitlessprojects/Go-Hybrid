//! Receive-operand soundness guards in `recv.rs` (`HYB-E2008`):
//! 1. `Expr::AddrOf` / `UnOp::Addr` projections (`(&arr)[0]`, `(&b).ch`) and composite `Call`
//!    projections (`.pop()`, `.unwrap()`, `.or()`, `some()`, callable channel results) in `recv_chan_type_expr`.
//! 2. `Stmt::If` init `composite_type_expr` fallback (`if sub := append(chs, ch); ...`) and comma-ok fallbacks.
//! 3. `Stmt::Match` guard `some(b)` / `option.some(b)` binders and `composite_type_expr` subject fallback.
use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn assert_e2008(src: &str) {
    let r = check_file(&parse_file("t.hy", src).expect("snippet parses"));
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{src}\ngot: {:?}", r.diagnostics);
    let e2008 = r.diagnostics.iter().filter(|d| d.code == "HYB-E2008").count();
    assert_eq!(e2008, 1, "expected exactly 1 HYB-E2008, got {:?} for:\n{src}", r.diagnostics);
}

fn assert_accept(src: &str) {
    let r = check_file(&parse_file("t.hy", src).expect("snippet parses"));
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept, got {:?} for:\n{src}", r.diagnostics);
    assert!(r.diagnostics.is_empty(), "expected 0 diagnostics, got {:?}", r.diagnostics);
}

const PRE: &str = "struct box_ch { ch chan<- int }\nstruct box_bi { ch chan int }\nstruct gen_box[t] { ch t }\nfn (b gen_box[t]) get() t { return b.ch }\nfn id_ch[t](x t) t { return x }\nenum pkt { empty, send_only(chan<- int), num(int), bi(chan int) }";

#[test]
fn addrof_elided_slice_and_composite_call_send_only_receives_reject_e2008() {
    for body in [
        "var arr [1]chan<- int\nvar a [<-(&arr)[0]]int\n_ = arr\n_ = a",
        "var b box_ch\nvar a [<-(&b).ch]int\n_ = b\n_ = a",
        "var chs []chan<- int\nvar ch chan<- int\nvar a [<-append(chs, ch).pop()]int\n_ = a",
        "var chs []chan<- int\nvar a [<-chs[0:1].pop()]int\n_ = a",
        "var chs []chan<- int\nvar a [<-some(chs[0:1]).unwrap()[0]]int\n_ = a",
        "var opt_ch chan option[chan<- int]\nvar a [<-(<-opt_ch).unwrap()]int\n_ = a",
        "var chs []chan<- int\nvar a [<-none.or(chs[0:1])[0]]int\n_ = a",
        "var fn_ch chan fn() chan<- int\nvar a [<-(<-fn_ch)()]int\n_ = a",
        "var ch chan<- int\nvar a [<-id_ch(ch)]int\n_ = a",
        "var chs []chan<- int\nvar a [<-id_ch(chs)[0]]int\n_ = a",
        "var gb gen_box[chan<- int]\nvar a [<-gb.get()]int\n_ = a",
    ] {
        assert_e2008(&format!("{PRE}\nfn main() {{\n{body}\n}}\n"));
    }
}

#[test]
fn if_init_and_match_guard_receive_operands_reject_e2008() {
    for body in [
        "var chs []chan<- int\nvar ch chan<- int\nif sub := append(chs, ch); len([<-sub[0]]int{}) > 0 {}",
        "var chs []chan<- int\nif sub := chs[0:1]; len([<-sub[0]]int{}) > 0 {}",
        "var ch_of_send chan chan<- int\nif ch, ok := <-ch_of_send; ok && len([<-ch]int{}) > 0 {}",
        "var maps []map[string]chan<- int\nif ch, ok := maps[0:1][0][\"k\"]; ok && len([<-ch]int{}) > 0 {}",
        "x := make(chan int, 1)\nvar opt option[int]\nmatch opt {\n    option.some(x) if (<-x) > 0 => {}\n    _ => {}\n}\n_ = x",
        "var opt option[chan<- int]\nmatch opt {\n    option.some(ch) if len([<-ch]int{}) > 0 => {}\n    _ => {}\n}",
        "var opts []option[chan<- int]\nmatch opts[0:1][0] {\n    some(ch) if len([<-ch]int{}) > 0 => {}\n    _ => {}\n}",
        "var opt_ch chan option[chan<- int]\nmatch <-opt_ch {\n    some(ch) if len([<-ch]int{}) > 0 => {}\n    _ => {}\n}",
        "var p pkt\nmatch p {\n    pkt.send_only(ch) if len([<-ch]int{}) > 0 => {}\n    _ => {}\n}",
        "ch := make(chan int, 1)\nvar p pkt\nmatch p {\n    pkt.num(ch) if (<-ch) > 0 => {}\n    _ => {}\n}\n_ = ch",
    ] {
        assert_e2008(&format!("{PRE}\nfn main() {{\n{body}\n}}\n"));
    }
}

#[test]
fn bidirectional_composite_if_init_and_match_guard_receives_stay_accepted() {
    for body in [
        "var arr [1]chan int\nvar a [<-(&arr)[0]]int\n_ = arr\n_ = a",
        "var b box_bi\nvar a [<-(&b).ch]int\n_ = b\n_ = a",
        "var chs []chan int\nvar ch chan int\nif sub := append(chs, ch); (<-sub[0]) > 0 {}",
        "var opt option[chan int]\nmatch opt {\n    some(ch) if (<-ch) > 0 => {}\n    some(_) => {}\n    none => {}\n}",
        "var opts []option[chan int]\nmatch opts[0:1][0] {\n    option.some(ch) if (<-ch) > 0 => {}\n    _ => {}\n}",
        "var p pkt\nmatch p {\n    pkt.bi(ch) if (<-ch) > 0 => {}\n    _ => {}\n}",
    ] {
        assert_accept(&format!("{PRE}\nfn main() {{\n{body}\n}}\n"));
    }
}
