use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check_codes(src: &str) -> (Verdict, Vec<String>) {
    let ast = parse_file("test.hy", &src.replace("; ", "\n")).expect("must parse");
    let r = check_file(&ast);
    let codes = r.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.code.clone()).collect();
    (r.verdict, codes)
}

#[test]
fn const_overflow_and_conv_alias_soundness() {
    for src in [
        "const f = 2.5; fn main() { var x int = f; fmt.println(x) }",
        "const f = 2.5; fn main() { var x int8 = 0; x = f; fmt.println(x) }",
        "fn main() { var x int = 1.25 + 1.25; fmt.println(x) }",
        "fn main() { var x int8 = 0; x = 1.5 * 3.0; fmt.println(x) }",
        "const big_f = 300.0; fn main() { var x int8 = big_f; fmt.println(x) }",
        "fn main() { var x int8 = 150.0 + 150.0; fmt.println(x) }",
        "fn main() { var x int = float64(2.5); fmt.println(x) }",
        "fn main() { var x float32 = float64(1e40); fmt.println(x) }",
        "fn main() { const big = 300; var x int8 = big; fmt.println(x) }",
        "fn main() { const big = 300; var x int8 = 0; x = big; fmt.println(x) }",
        "fn main() { const huge = 1 << 70; x := huge; fmt.println(x) }",
        "fn main() { const big_f = 1e40; var y float32 = big_f; fmt.println(y) }",
        "fn main() { const frac = 2.5; var x int = frac; fmt.println(x) }",
        "fn main() { var x int8 = len(\"hello\") + 125; fmt.println(x) }",
        "const s = \"hello\"; fn main() { var x int8 = len(s) + 125; fmt.println(x) }",
    ] {
        let (v, codes) = check_codes(src);
        assert!(v == Verdict::Reject && codes.iter().any(|c| c == "HYB-E6002"), "expected HYB-E6002 for: {src}");
    }
    for src in [
        "fn main() { x := (int8)(2.5); fmt.println(x) }",
        "fn main() { x := (int)(\"abc\"); fmt.println(x) }",
        "fn main() { x := ([]int)(3); fmt.println(x) }",
        "fn main() { x := (float32)(1e40); fmt.println(x) }",
        "type my_i8 = int8; fn main() { x := my_i8(2.5); fmt.println(x) }",
        "type my_i8 = int8; fn main() { x := my_i8(300); fmt.println(x) }",
        "type my_f32 = float32; fn main() { x := my_f32(1e40); fmt.println(x) }",
        "type my_str = string; fn main() { x := my_str(2.5); fmt.println(x) }",
        "type my_ints = []int; fn main() { x := my_ints(3); fmt.println(x) }",
        "type my_int = int; fn main() { x := []my_int(\"abc\"); fmt.println(x) }",
        "fn main() { x := bool(1); fmt.println(x) }",
        "fn main() { x := bool(\"true\"); fmt.println(x) }",
        "fn main() { x := char(2.5); fmt.println(x) }",
        "fn main() { x := char(\"abc\"); fmt.println(x) }",
        "fn main() { x := char(99999999999); fmt.println(x) }",
        "const s = \"abc\"; fn main() { x := int(s); fmt.println(x) }",
        "const f = 2.5; fn main() { x := int8(f); fmt.println(x) }",
        "const b = true; fn main() { x := int8(b); fmt.println(x) }",
        "const c = 300; fn main() { x := int8(c); fmt.println(x) }",
        "fn main() { const c = 300; x := int8(c); fmt.println(x) }",
        "fn main() { x := int8(1.5 + 1.0); fmt.println(x) }",
        "fn main() { x := float32(1e30 * 1e20); fmt.println(x) }",
        "fn main() { x := string(1 == 2); fmt.println(x) }",
        "fn main() { x := []int(1 + 2); fmt.println(x) }",
        "fn main() { x := int(\"a\" + \"b\"); fmt.println(x) }",
    ] {
        let (v, codes) = check_codes(src);
        assert!(v == Verdict::Reject && codes.iter().any(|c| c == "HYB-E6006"), "expected HYB-E6006 for: {src}");
    }
    for src in [
        "type bytes = []byte; fn main() { b := bytes(\"abc\"); fmt.println(len(b)) }",
        "fn int8(x float64) int { return 0 }; fn main() { x := int8(2.5); fmt.println(x) }",
        "fn main() { const ok = 100; var x int8 = ok; y := int8(ok); fmt.println(x, y) }",
        "const c = 300; fn main() { c := 10; x := int8(c); fmt.println(x) }",
        "type my_char = char; fn main() { b := bool(true); c := my_char('A'); fmt.println(b, c) }",
    ] {
        assert_eq!(check_codes(src).0, Verdict::Accept, "expected Accept for: {src}");
    }
    for src in [
        "type my_byte = byte; fn main() { b := []my_byte(\"abc\"); fmt.println(len(b)) }",
        "type my_rune = rune; fn main() { r := []my_rune(\"abc\"); fmt.println(len(r)) }",
        "fn int8(s string) int { return len(s) }; fn main() { x := int8(\"abc\"); fmt.println(x) }",
    ] {
        assert!(!check_codes(src).1.iter().any(|c| c == "HYB-E6006"), "unexpected HYB-E6006 for: {src}");
    }
}
