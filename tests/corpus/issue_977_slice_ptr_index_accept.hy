// EXPECTED-VERDICT: ACCEPT
// Issue #977 (B): hy-emit does not double-address an already-pointer slice
// index expression (`take(q[0])` where `q` is `[]*inner`).

struct inner {
    k int
}

fn take(v *inner) int {
    return v.k
}

fn main() {
    v := inner{k: 7}
    q := []*inner{&v}
    fmt.println("{take(q[0])}")
}
