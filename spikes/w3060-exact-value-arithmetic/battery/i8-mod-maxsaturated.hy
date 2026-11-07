// W-3060 battery: `127 % 127` on typed i8 variables (max-saturated draw).
fn main() {
    var a i8 = 127
    var b i8 = 127
    c := a % b
    fmt.println(c)
}
