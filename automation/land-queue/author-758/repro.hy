const zero = 0

fn main() {
    x := 10 / zero
    _ = 1.0 / (1.0 - 1.0)
    y := 10 / (len("abc") - 3)
    var a [1 / 0]int
    _ = x
    _ = y
    _ = a
}
