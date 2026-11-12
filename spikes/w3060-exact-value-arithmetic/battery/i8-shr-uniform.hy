// W-3060 battery: `-51 >> 5` on typed i8 variables (uniform draw).
fn main() {
    var a i8 = -51
    var b i8 = 5
    c := a >> b
    fmt.println(c)
}
