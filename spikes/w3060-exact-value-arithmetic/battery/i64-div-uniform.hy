// W-3060 battery: `-9223372036854775808 / -1` on typed i64 variables (uniform draw).
fn main() {
    var a i64 = -9223372036854775808
    var b i64 = -1
    c := a / b
    fmt.println(c)
}
