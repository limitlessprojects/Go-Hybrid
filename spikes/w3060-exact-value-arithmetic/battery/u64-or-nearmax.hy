// W-3060 battery: `18446744073709551615 | 2` on typed u64 variables (near-max draw).
fn main() {
    var a u64 = 18446744073709551615
    var b u64 = 2
    c := a | b
    fmt.println(c)
}
