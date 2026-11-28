// W-3060 battery: `-94 << 0` on typed i8 variables (uniform draw).
fn main() {
    var a i8 = -94
    var b i8 = 0
    c := a << b
    fmt.println(c)
}
