// W-3060 battery: `47 >> 7` on typed u8 variables (uniform draw).
fn main() {
    var a u8 = 47
    var b u8 = 7
    c := a >> b
    fmt.println(c)
}
