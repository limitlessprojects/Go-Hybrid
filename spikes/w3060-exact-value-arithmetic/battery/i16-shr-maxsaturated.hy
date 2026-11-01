// W-3060 battery: `32767 >> 15` on typed i16 variables (max-saturated draw).
fn main() {
    var a i16 = 32767
    var b i16 = 15
    c := a >> b
    fmt.println(c)
}
