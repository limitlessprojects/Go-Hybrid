// W-3060 battery: `65535 / 65535` on typed u16 variables (max-saturated draw).
fn main() {
    var a u16 = 65535
    var b u16 = 65535
    c := a / b
    fmt.println(c)
}
