use hy_check::check_file;
use hy_syntax::parse_file;

#[test]
fn type_use_shadowed_by_local_value_rejects_e1002() {
    for src in [
        "fn main() { int := 1\n var y int = 2\n _ = int\n _ = y }",
        "fn f(int int) int { var y int = int\n return y }\nfn main() { _ = f(1) }",
        "fn main() { bool := 1\n xs := []bool{true}\n _ = bool\n _ = xs }",
        "struct p { x int }\nfn main() { p := p{x: 1}\n q := p{x: 2}\n _ = p.x + q.x }",
    ] {
        let d = check_file(&parse_file("t.hy", src).unwrap()).diagnostics;
        assert!(d.iter().any(|x| x.code == "HYB-E1002"), "{src}: {d:?}");
    }
}
