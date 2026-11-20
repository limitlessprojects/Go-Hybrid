use hy_check::check_file;
use hy_syntax::parse_file;

#[test]
fn const_conversions_to_bool_complex_and_array_reject_e6006() {
    for expr in [
        "bool(1)",
        "bool(\"true\")",
        "complex64(\"hello\")",
        "complex128(true)",
        "c64(nil)",
        "complex64(1e40)",
        "[4]byte(\"text\")",
        "[4]int(nil)",
    ] {
        let src = format!("fn main() {{\n    _ = {expr}\n}}\n");
        let d = check_file(&parse_file("t.hy", &src).unwrap()).diagnostics;
        assert!(d.iter().any(|x| x.code == "HYB-E6006"), "{expr}: {d:?}");
    }
}
