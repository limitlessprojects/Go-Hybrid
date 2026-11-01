// W-3060 battery: `-32768 % -1` on typed i16 variables (uniform draw).
fn main() {
    var a i16 = -32768
    var b i16 = -1
    c := a % b
    fmt.println(c)
}
