// W-3060 battery: `7461 >> 5` on typed u16 variables (uniform draw).
fn main() {
    var a u16 = 7461
    var b u16 = 5
    c := a >> b
    fmt.println(c)
}
