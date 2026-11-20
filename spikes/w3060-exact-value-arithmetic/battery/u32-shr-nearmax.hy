// W-3060 battery: `4294967295 >> 2` on typed u32 variables (near-max draw).
fn main() {
    var a u32 = 4294967295
    var b u32 = 2
    c := a >> b
    fmt.println(c)
}
