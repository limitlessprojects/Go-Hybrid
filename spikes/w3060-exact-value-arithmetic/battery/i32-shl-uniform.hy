// W-3060 battery: `2034692552 << 19` on typed i32 variables (uniform draw).
fn main() {
    var a i32 = 2034692552
    var b i32 = 19
    c := a << b
    fmt.println(c)
}
