// W-3060 battery: `18446744073709551615 / 18446744073709551615` on typed u64 variables (max-saturated draw).
fn main() {
    var a u64 = 18446744073709551615
    var b u64 = 18446744073709551615
    c := a / b
    fmt.println(c)
}
