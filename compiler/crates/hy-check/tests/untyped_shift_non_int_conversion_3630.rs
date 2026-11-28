use hy_check::check_file;
use hy_syntax::parse_file;

#[test]
fn untyped_shift_non_int_conversion_rejects_e1002() {
    for src in [
        // 1. float64(1 << n)
        "fn main() {\n n := 3\n f := float64(1 << n)\n _ = f\n}",
        // 2. string(1 << n)
        "fn main() {\n n := 3\n s := string(1 << n)\n _ = s\n}",
        // 3. var f float64 = float64(1 << n)
        "fn main() {\n n := 3\n var f float64 = float64(1 << n)\n _ = f\n}",
        // 4. float64((1 << n) + 2)
        "fn main() {\n n := 3\n f := float64((1 << n) + 2)\n _ = f\n}",
        // 5. float64(-(1 << n))
        "fn main() {\n n := 3\n f := float64(-(1 << n))\n _ = f\n}",
        // 6. bool(1 << n)
        "fn main() {\n n := 3\n b := bool(1 << n)\n _ = b\n}",
    ] {
        let d = check_file(&parse_file("test.hy", src).unwrap()).diagnostics;
        assert!(d.iter().any(|x| x.code == "HYB-E1002"), "{src}: {d:?}");
    }
}

#[test]
fn valid_shifts_in_conversions_accepted() {
    for src in [
        // Constant shift folded: float64(1 << 3)
        "fn main() {\n f := float64(1 << 3)\n _ = f\n}",
        // Typed operand: float64(int(1) << n)
        "fn main() {\n n := 3\n f := float64(int(1) << n)\n _ = f\n}",
        // Typed variable: float64(one << n)
        "fn main() {\n n := 3\n var one int = 1\n f := float64(one << n)\n _ = f\n}",
        // Const shift: float64(1 << k)
        "const k int = 3\nfn main() {\n f := float64(1 << k)\n _ = f\n}",
        // Shift inside integer conversion
        "fn main() {\n n := 3\n i := int(1 << n)\n _ = i\n}",
        "fn main() {\n n := 3\n u := uint64(1 << n)\n _ = u\n}",
    ] {
        let d = check_file(&parse_file("test.hy", src).unwrap()).diagnostics;
        assert!(!d.iter().any(|x| x.code == "HYB-E1002"), "{src}: {d:?}");
    }
}
