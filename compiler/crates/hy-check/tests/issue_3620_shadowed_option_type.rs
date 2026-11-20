use hy_check::check_file;
use hy_syntax::parse_file;

#[test]
fn option_type_shadowed_by_local_value_rejects_e1002() {
    for src in [
        "fn main() { option := 1\n var o option[int] = some(1)\n _ = o\n _ = option }",
        "fn f(option int) int { var o option[int] = some(1)\n _ = o\n return option }\nfn main() { _ = f(1) }",
        "fn main() { option := 1\n xs := []option[int]{some(1)}\n _ = xs\n _ = option }",
        "fn main() { option := 1\n m := map[string]option[int]{}\n _ = m\n _ = option }",
    ] {
        let d = check_file(&parse_file("t.hy", src).unwrap()).diagnostics;
        assert!(d.iter().any(|x| x.code == "HYB-E1002"), "{src}: {d:?}");
    }
}

#[test]
fn option_type_unshadowed_accepts_cleanly() {
    for src in [
        "fn main() { var o option[int] = some(1)\n _ = o }",
        "fn f(x int) option[int] { return some(x) }\nfn main() { _ = f(1) }",
    ] {
        let d = check_file(&parse_file("t.hy", src).unwrap()).diagnostics;
        assert!(d.is_empty(), "{src}: {d:?}");
    }
}
