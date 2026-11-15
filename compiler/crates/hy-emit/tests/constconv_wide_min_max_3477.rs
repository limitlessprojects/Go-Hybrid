use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn constconv_preserves_512bit_wideint_in_min_and_max_3477() {
    let src = "fn main() {\n    a := u8(min(1 << 130, 300))\n    b := u8((max(1 << 130, 300) >> 122) + 44)\n    fmt.println(a, b)\n}\n";
    let ast = parse_file("test.hy", src).expect("parses");
    let go = emit_go(&ast).expect("emits");
    assert!(go.contains("a := uint8(44)"), "{go}");
    assert!(go.contains("b := uint8(44)"), "{go}");
}
