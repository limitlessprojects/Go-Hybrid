// W-3060 battery: `6001128144056537569 >> 10` on typed u64 variables (uniform draw).
fn main() {
    var a u64 = 6001128144056537569
    var b u64 = 10
    c := a >> b
    fmt.println(c)
}
