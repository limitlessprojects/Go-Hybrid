// W-3060 battery: `255 % 2` on typed u8 variables (near-max draw).
fn main() {
    var a u8 = 255
    var b u8 = 2
    c := a % b
    fmt.println(c)
}
