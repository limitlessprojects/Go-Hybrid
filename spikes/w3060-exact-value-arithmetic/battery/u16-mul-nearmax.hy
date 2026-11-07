// W-3060 battery: `65535 * 2` on typed u16 variables (near-max draw).
fn main() {
    var a u16 = 65535
    var b u16 = 2
    c := a * b
    fmt.println(c)
}
