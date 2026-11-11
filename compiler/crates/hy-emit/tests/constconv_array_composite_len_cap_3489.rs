use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn constconv_folds_len_and_cap_on_array_literals_3489() {
    let src = "fn main() {\n    a := u8(len([300]int{}))\n    b := u8(cap([300]int{}))\n    c := i8(len([100 + 100]int{}))\n    d := u8(len([...]int{1, 2, 3}) + 297)\n    fmt.println(a, b, c, d)\n}\n";
    let ast = parse_file("test.hy", src).expect("parses");
    let go = emit_go(&ast).expect("emits");
    assert!(go.contains("a := uint8(44)") && go.contains("b := uint8(44)"), "{go}");
    assert!(go.contains("c := int8(-56)") && go.contains("d := uint8(44)"), "{go}");
}
