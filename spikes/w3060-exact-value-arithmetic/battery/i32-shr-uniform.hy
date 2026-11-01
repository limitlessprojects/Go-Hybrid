// W-3060 battery: `-856378971 >> 10` on typed i32 variables (uniform draw).
fn main() {
    var a i32 = -856378971
    var b i32 = 10
    c := a >> b
    fmt.println(c)
}
