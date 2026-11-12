use hy_check::check_file;
use hy_syntax::parse_file;

#[test]
fn const_to_array_conversion_in_var_rejects() {
    for e in ["[4]int(123)", "[4]int(true)", "[4]byte(\"text\")", "[4]int(nil)"] {
        let src = format!("fn main() {{\n    var x = {e}\n    println(x[0])\n}}\n");
        let d = check_file(&parse_file("t.hy", &src).unwrap()).diagnostics;
        assert!(
            d.iter().any(|x| x.code == "HYB-E6006" && x.message.contains("[4]")),
            "{e}: {d:?}"
        );
    }
}
