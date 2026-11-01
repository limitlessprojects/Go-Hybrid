// W-3060 battery: `160 << 3` on typed u8 variables (uniform draw).
fn main() {
    var a u8 = 160
    var b u8 = 3
    c := a << b
    fmt.println(c)
}
