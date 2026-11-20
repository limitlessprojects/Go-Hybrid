// W-3060 battery: `9223372036854775807 - 9223372036854775807` on typed i64 variables (max-saturated draw).
fn main() {
    var a i64 = 9223372036854775807
    var b i64 = 9223372036854775807
    c := a - b
    fmt.println(c)
}
