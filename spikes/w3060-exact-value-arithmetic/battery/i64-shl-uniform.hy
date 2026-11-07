// W-3060 battery: `5747508038358800218 << 32` on typed i64 variables (uniform draw).
fn main() {
    var a i64 = 5747508038358800218
    var b i64 = 32
    c := a << b
    fmt.println(c)
}
