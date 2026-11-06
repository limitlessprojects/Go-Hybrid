use hy_check::check_file;
use hy_syntax::parse_file;

#[test]
fn int_const_overflow_in_any_slots_rejects_e6002() {
    for src in [
        // 1. fmt.println(1 << 64)
        "fn main() {\n fmt.println(1 << 64)\n}",
        // 2. fmt.println(big * 4)
        "const big = 1 << 62\nfn main() {\n fmt.println(big * 4)\n}",
        // 3. fmt.println(big + big + big)
        "const big = 1 << 62\nfn main() {\n fmt.println(big + big + big)\n}",
        // 4. fmt.println(18446744073709551615)
        "fn main() {\n fmt.println(18446744073709551615)\n}",
        // 5. var a any = big * 4
        "const big = 1 << 62\nfn main() {\n var a any = big * 4\n _ = a\n}",
        // 6. s := []any{big * 4}
        "const big = 1 << 62\nfn main() {\n s := []any{big * 4}\n _ = s\n}",
        // 7. fmt.printf with overflowing argument
        "fn main() {\n fmt.printf(\"%v\", 1 << 64)\n}",
    ] {
        let d = check_file(&parse_file("t.hy", src).unwrap()).diagnostics;
        assert!(d.iter().any(|x| x.code == "HYB-E6002"), "{src}: {d:?}");
    }
}

#[test]
fn valid_int_const_in_any_slots_accepted() {
    for src in [
        "fn main() {\n fmt.println(42)\n}",
        "const small = 100\nfn main() {\n fmt.println(small * 2)\n}",
        "fn main() {\n var a any = 10\n _ = a\n}",
        "fn main() {\n s := []any{1, 2, 3}\n _ = s\n}",
    ] {
        let d = check_file(&parse_file("t.hy", src).unwrap()).diagnostics;
        assert!(!d.iter().any(|x| x.code == "HYB-E6002"), "{src}: {d:?}");
    }
}
