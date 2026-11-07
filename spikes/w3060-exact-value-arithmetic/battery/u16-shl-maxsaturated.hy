// W-3060 battery: `65535 << 15` on typed u16 variables (max-saturated draw).
fn main() {
    var a u16 = 65535
    var b u16 = 15
    c := a << b
    fmt.println(c)
}
