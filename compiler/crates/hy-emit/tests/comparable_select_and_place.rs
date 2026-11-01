//! `k` as a map key or set element only in a select arm, assignment place,
//! or array length must emit `[k comparable]` (Row 13 BUILDABLE). Skipping
//! those positions emitted `[k any]` and `go build` rejected the key.

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let ast = parse_file("t.hy", src).expect("parses");
    emit_go(&ast).expect("emits")
}

#[test]
fn comparable_from_select_place_and_array_len() {
    let go = emit(
        "fn send_val[k](ch chan int) { select { ch <- len(map[k]int{}): {} default: {} } }\n\
         fn send_ch[k](chs []chan int) { select { chs[len(set[k]{})] <- 1: {} default: {} } }\n\
         fn recv_ch[k](chs []chan int) { select { v := <-chs[len(map[k]int{})]: {} default: {} } }\n\
         fn poke[k](xs []int) { xs[len(map[k]int{})] = 1 }\n\
         fn poke_addr[k](xs []int) { &xs[len(set[k]{})] = xs }\n\
         fn poke_field[k](r int) { r.cells[len(map[k]int{})].n = 1 }\n\
         struct grid[k] { cells [len(set[k]{})]int }\n\
         fn id[k](x k) k { return x }\n",
    );
    for sig in [
        "func send_val[k comparable]",
        "func send_ch[k comparable]",
        "func recv_ch[k comparable]",
        "func poke[k comparable]",
        "func poke_addr[k comparable]",
        "func poke_field[k comparable]",
        "type grid[k comparable] struct",
    ] {
        assert!(go.contains(sig), "missing `{sig}` in:\n{go}");
    }
    assert!(
        go.contains("func id[k any]"),
        "plain param must stay any:\n{go}"
    );
}
