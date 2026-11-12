// W-3060 battery: `32767 + 2` on typed i16 variables (near-max draw).
fn main() {
    var a i16 = 32767
    var b i16 = 2
    c := a + b
    fmt.println(c)
}
