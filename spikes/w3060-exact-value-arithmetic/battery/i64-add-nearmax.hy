// W-3060 battery: `9223372036854775807 + 2` on typed i64 variables (near-max draw).
fn main() {
    var a i64 = 9223372036854775807
    var b i64 = 2
    c := a + b
    fmt.println(c)
}
