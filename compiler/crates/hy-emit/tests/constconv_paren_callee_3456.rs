use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn constconv_folds_parenthesized_callees_3456() {
    let src = "fn main() {\n    a := u8((u8)(10) + 250)\n    b := u8((len)(\"hello\") + 255)\n    c := u8((min)(300, 400))\n    fmt.println(a, b, c)\n}\n";
    let ast = parse_file("test.hy", src).expect("parses");
    let go = emit_go(&ast).expect("emits");
    assert!(go.contains("a := uint8(4)"), "{go}");
    assert!(go.contains("b := uint8(4)"), "{go}");
    assert!(go.contains("c := uint8(44)"), "{go}");
}
