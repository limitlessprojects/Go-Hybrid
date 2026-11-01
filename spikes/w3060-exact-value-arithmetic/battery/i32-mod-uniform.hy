// W-3060 battery: `-2147483648 % -1` on typed i32 variables (uniform draw).
fn main() {
    var a i32 = -2147483648
    var b i32 = -1
    c := a % b
    fmt.println(c)
}
