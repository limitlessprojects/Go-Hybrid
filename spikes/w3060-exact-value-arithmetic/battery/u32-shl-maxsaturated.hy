// W-3060 battery: `4294967295 << 31` on typed u32 variables (max-saturated draw).
fn main() {
    var a u32 = 4294967295
    var b u32 = 31
    c := a << b
    fmt.println(c)
}
