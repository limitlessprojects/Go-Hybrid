// W-3060 battery: `-19193 >> 7` on typed i16 variables (uniform draw).
fn main() {
    var a i16 = -19193
    var b i16 = 7
    c := a >> b
    fmt.println(c)
}
