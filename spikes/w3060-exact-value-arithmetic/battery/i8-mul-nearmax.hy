// W-3060 battery: `127 * 2` on typed i8 variables (near-max draw).
fn main() {
    var a i8 = 127
    var b i8 = 2
    c := a * b
    fmt.println(c)
}
