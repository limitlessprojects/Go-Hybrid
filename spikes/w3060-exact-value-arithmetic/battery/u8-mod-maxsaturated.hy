// W-3060 battery: `255 % 255` on typed u8 variables (max-saturated draw).
fn main() {
    var a u8 = 255
    var b u8 = 255
    c := a % b
    fmt.println(c)
}
