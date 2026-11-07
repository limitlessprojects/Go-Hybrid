// W-3060 battery: `835455006 >> 0` on typed u32 variables (uniform draw).
fn main() {
    var a u32 = 835455006
    var b u32 = 0
    c := a >> b
    fmt.println(c)
}
