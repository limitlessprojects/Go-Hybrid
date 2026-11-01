// W-3060 battery: `4161289226949855977 << 10` on typed u64 variables (uniform draw).
fn main() {
    var a u64 = 4161289226949855977
    var b u64 = 10
    c := a << b
    fmt.println(c)
}
