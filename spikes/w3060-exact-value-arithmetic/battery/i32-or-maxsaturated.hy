// W-3060 battery: `2147483647 | 2147483647` on typed i32 variables (max-saturated draw).
fn main() {
    var a i32 = 2147483647
    var b i32 = 2147483647
    c := a | b
    fmt.println(c)
}
