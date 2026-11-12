// W-3060 battery: `41626 << 9` on typed u16 variables (uniform draw).
fn main() {
    var a u16 = 41626
    var b u16 = 9
    c := a << b
    fmt.println(c)
}
