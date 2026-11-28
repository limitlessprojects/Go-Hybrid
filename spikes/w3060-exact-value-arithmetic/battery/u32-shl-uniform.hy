// W-3060 battery: `3844259697 << 30` on typed u32 variables (uniform draw).
fn main() {
    var a u32 = 3844259697
    var b u32 = 30
    c := a << b
    fmt.println(c)
}
