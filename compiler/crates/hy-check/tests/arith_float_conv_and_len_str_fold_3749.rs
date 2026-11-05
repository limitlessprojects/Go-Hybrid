use hy_check::{Verdict, check_file};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let parsed = parse_file("test.hy", src);
    assert!(
        parsed.diagnostics.is_empty(),
        "parse failed: {:?}",
        parsed.diagnostics
    );
    check_file(&parsed.file)
}

#[test]
fn rejects_nested_and_named_float_conversion_zero_divisors_and_dividends_3749() {
    let r = check(
        "package main
         const fz = float64(0)
         const num = float64(5)
         fn main() {
             a := 1.0 / -float64(0)
             b := 1.0 / (float64(1) - float64(1))
             c := 1.0 / (f64(len("abc")) - 3.0)
             d := 1.0 / fz
             e := float64(1) / 0.0
             f := num / 0.0
             _ = a
             _ = b
             _ = c
             _ = d
             _ = e
             _ = f
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    let e6001 = r.diagnostics.iter().filter(|d| d.code == "HYB-E6001").count();
    assert_eq!(e6001, 6, "{:?}", r.diagnostics);
}

#[test]
fn rejects_int_converted_len_and_chained_or_concatenated_string_consts_3749() {
    let r = check(
        "package main
         const a = ""
         const b = a
         const s1 = "ab"
         const s2 = s1 + "c"
         fn main() {
             x1 := 10 / int(len(""))
             x2 := 10 % int64(len("abc") - 3)
             x3 := 10 / u8(len("") + 256)
             x4 := 10 / len(b)
             x5 := 10 / len("" + "")
             x6 := 10 / (len(s2) - 3)
             _ = x1
             _ = x2
             _ = x3
             _ = x4
             _ = x5
             _ = x6
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    let e6001 = r.diagnostics.iter().filter(|d| d.code == "HYB-E6001").count();
    assert_eq!(e6001, 6, "{:?}", r.diagnostics);
}

#[test]
fn accepts_shadowed_float_and_int_conversion_divisors_3749() {
    let r = check(
        "package main
         const c = 1.0
         const zero = 0
         fn main() {
             f1 := fn(c float64) float64 {
                 return c / 0.0
             }
             f2 := fn(zero int) int {
                 return 10 / int(zero)
             }
             int := fn(x int) int {
                 return x + 2
             }
             v := 10 / int(0)
             _ = f1
             _ = f2
             _ = v
         }",
    );
    let e6001: Vec<_> = r.diagnostics.iter().filter(|d| d.code == "HYB-E6001").collect();
    assert!(e6001.is_empty(), "{:?}", e6001);
}
