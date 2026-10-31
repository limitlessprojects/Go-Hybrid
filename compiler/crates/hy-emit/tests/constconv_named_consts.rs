use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn folds_file_and_local_consts_in_truncating_conversions_preserving_shadows() {
    let src = "\
const big = 300
fn main() {
    const loc = 256 + 44
    a := u8(big)
    b := u8(loc)
    c := i8(big - 100)
    big := 7
    d := u8(big)
    fmt.println(a, b, c, d)
}
";
    let go = emit_go(&parse_file("t.hy", src).unwrap()).expect("emit");
    assert!(go.contains("a := uint8(44)"), "go:\n{go}");
    assert!(go.contains("b := uint8(44)"), "go:\n{go}");
    assert!(go.contains("c := int8(-56)"), "go:\n{go}");
    assert!(go.contains("d := uint8(big)"), "shadowed var must stay runtime:\n{go}");
}
