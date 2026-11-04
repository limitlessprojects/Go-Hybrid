use hy_check::check_file;
use hy_syntax::parse_file;

#[test]
fn typed_const_literal_kind_mismatch_rejects_e1002() {
    for src in [
        // 1. const k string = 1
        "const k string = 1\nfn main() {\n _ = k\n}",
        // 2. const k string = true
        "const k string = true\nfn main() {\n _ = k\n}",
        // 3. const k string = 1.5
        "const k string = 1.5\nfn main() {\n _ = k\n}",
        // 4. const k string = 'a'
        "const k string = 'a'\nfn main() {\n _ = k\n}",
        // 5. const k int = "x"
        "const k int = \"x\"\nfn main() {\n _ = k\n}",
        // 6. const k int = true
        "const k int = true\nfn main() {\n _ = k\n}",
        // 7. const k bool = 1
        "const k bool = 1\nfn main() {\n _ = k\n}",
        // 8. const k bool = "x"
        "const k bool = \"x\"\nfn main() {\n _ = k\n}",
        // 9. const k float64 = "x"
        "const k float64 = \"x\"\nfn main() {\n _ = k\n}",
        // 10. const k float64 = true
        "const k float64 = true\nfn main() {\n _ = k\n}",
        // 11. const k rune = "x"
        "const k rune = \"x\"\nfn main() {\n _ = k\n}",
        // 12. const k byte = "x"
        "const k byte = \"x\"\nfn main() {\n _ = k\n}",
        // 13. const k uint8 = true
        "const k uint8 = true\nfn main() {\n _ = k\n}",
    ] {
        let d = check_file(&parse_file("test.hy", src).unwrap()).diagnostics;
        assert!(d.iter().any(|x| x.code == "HYB-E1002"), "{src}: {d:?}");
    }
}

#[test]
fn typed_const_valid_kinds_accepted() {
    for src in [
        "const k int = 42\nfn main() {\n _ = k\n}",
        "const k string = \"hello\"\nfn main() {\n _ = k\n}",
        "const k bool = true\nfn main() {\n _ = k\n}",
        "const k float64 = 3.14\nfn main() {\n _ = k\n}",
        "const k int = 'a'\nfn main() {\n _ = k\n}",
        "const k bool = 1 == 1\nfn main() {\n _ = k\n}",
    ] {
        let d = check_file(&parse_file("test.hy", src).unwrap()).diagnostics;
        assert!(!d.iter().any(|x| x.code == "HYB-E1002"), "{src}: {d:?}");
    }
}
