// W-3060 battery: `-128 / -1` on typed i8 variables (uniform draw).
fn main() {
    var a i8 = -128
    var b i8 = -1
    c := a / b
    fmt.println(c)
}
