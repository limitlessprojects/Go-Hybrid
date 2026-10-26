use super::fold_constant;

#[test]
fn test_constant_folding() {
    let operand = operand_of("fn main() { b := u8(300) }");
    assert_eq!(fold_constant(&operand, "u8"), Some(44));
}