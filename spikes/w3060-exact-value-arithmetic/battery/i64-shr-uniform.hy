// W-3060 battery: `7585309757922330237 >> 39` on typed i64 variables (uniform draw).
fn main() {
    var a i64 = 7585309757922330237
    var b i64 = 39
    c := a >> b
    fmt.println(c)
}
