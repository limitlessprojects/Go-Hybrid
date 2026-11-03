fn emit(src: &str) -> String {
    let file = hy_syntax::parse_file("t.hy", src).expect("parse");
    hy_emit::emit_go(&file).unwrap_or_else(|e| panic!("emit failed: {}", e.message))
}

#[test]
fn constconv_folds_len_and_nested_calls_3084() {
    let go = emit("fn main() {\n    a := u8(len(\"abc\") - 10)\n    b := u8(max(300, 10))\n    _ = a\n    _ = b\n}\n");
    assert!(go.contains("uint8(249)"), "{go}");
    assert!(go.contains("uint8(44)"), "{go}");
}

#[test]
fn constconv_folds_parenthesized_callees_3456() {
    let go = emit(
        "fn main() {\n    a := u8((u8)(10) + 250)\n    b := u8((len)(\"hello\") + 255)\n    c := u8((min)(300, 400))\n    _ = a\n    _ = b\n    _ = c\n}\n",
    );
    assert!(go.contains("uint8(4)"), "{go}");
    assert!(go.contains("uint8(44)"), "{go}");
}
