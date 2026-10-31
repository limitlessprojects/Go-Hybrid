// EXPECTED-VERDICT: ACCEPT
// Issue #977 (A): hy-emit auto-addresses addressable value arguments passed
// to pointer parameters on method callees (`b.take(v)` -> `b.take(&v)`).

struct box {
    n int
}

struct inner {
    k int
}

fn (b *box) take(v *inner) int {
    return b.n + v.k
}

fn main() {
    b := box{n: 1}
    v := inner{k: 2}
    fmt.println("{b.take(v)}")
}
