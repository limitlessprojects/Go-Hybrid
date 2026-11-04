use hy_check::check_file;
use hy_syntax::parse_file;

#[test]
fn option_type_use_shadowed_by_local_value_rejects_e1002() {
    for src in [
        // 1. local variable `option` shadows `option[int]`
        "fn main() {\n option := 1\n var o option[int] = some(1)\n _ = option\n _ = o }",
        // 2. parameter `option` shadows `option[int]`
        "fn f(option int) int {\n var o option[int] = some(1)\n _ = o\n return option }\nfn main() { _ = f(1) }",
        // 3. slice literal of option type with local `option`
        "fn main() {\n option := 1\n xs := []option[int]{some(1)}\n _ = option\n _ = xs }",
        // 4. map literal with option value and local `option`
        "fn main() {\n option := 1\n m := map[string]option[int]{\"a\": some(1)}\n _ = option\n _ = m }",
    ] {
        let d = check_file(&parse_file("t.hy", src).unwrap()).diagnostics;
        assert!(d.iter().any(|x| x.code == "HYB-E1002"), "{src}: {d:?}");
    }
}

#[test]
fn option_type_unshadowed_accepted() {
    for src in [
        "fn main() {\n var o option[int] = some(1)\n _ = o }",
        "fn f(x int) option[int] {\n return some(x) }\nfn main() { _ = f(1) }",
        // shadow in an inner block with option type used only after the block
        "fn main() {\n if true {\n option := 1\n _ = option\n }\n var o option[int] = some(1)\n _ = o }",
    ] {
        let d = check_file(&parse_file("t.hy", src).unwrap()).diagnostics;
        assert!(!d.iter().any(|x| x.code == "HYB-E1002"), "{src}: {d:?}");
    }
}
