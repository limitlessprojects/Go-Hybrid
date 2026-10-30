// EXPECTED-VERDICT: ACCEPT
// Issue #1003 control: valid assignments between values of the same type parameter T
// continue to be accepted by hy-check.

fn swap_pair[T any](a T, b T) (T, T) {
    mut x := a
    mut y := b
    tmp := x
    x = y
    y = tmp
    return x, y
}

fn main() {
    left, right := swap_pair[int](10, 20)
    fmt.println("{left} {right}")
}
