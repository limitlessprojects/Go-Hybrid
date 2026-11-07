//! Regression tests for `arith.rs` constant zero-divisor detection (`HYB-E6001`):
//! 1. Float conversions (`f32`/`f64`/`float32`/`float64`) inside unary negation,
//!    binary arithmetic, named constants, and constant float dividends must fold
//!    and reject with `HYB-E6001`.
//! 2. Integer conversions wrapping `len(...)` of constant strings (`10 / int(len(""))`,
//!    `10 % int64(len("abc") - 3)`, `10 / u8(len("") + 256)`) and chained/concatenated
//!    string constants (`const b = a`, `"" + ""`) must reject with `HYB-E6001`.
//! 3. Shadowed conversion callees, closure-shadowed conversion arguments, and
//!    closure-shadowed float dividends (`f := fn(c float64) float64 { return c / 0.0 }`)
//!    must remain accepted.

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("parses");
    check_file(&ast).diagnostics.into_iter().map(|d| d.code).collect()
}

fn assert_code(src: &str, want: &str) {
    let cs = codes(src);
    assert!(!cs.is_empty() && cs.iter().all(|c| c == want), "expected only [{want}], got {cs:?} for:\n{src}");
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(cs.is_empty(), "expected 0 diagnostics, got {cs:?} for:\n{src}");
}

#[test]
fn nested_and_named_float_conversion_zero_divisors_reject() {
    assert_code("fn main() {\n    _ = 1.0 / -float64(0)\n}\n", "HYB-E6001");
    assert_code("fn main() {\n    _ = 1.0 / (float64(1) - float64(1))\n}\n", "HYB-E6001");
    assert_code("fn main() {\n    _ = 1.0 / (f64(len(\"abc\")) - 3.0)\n}\n", "HYB-E6001");
    assert_code("const fz = float64(0)\n\nfn main() {\n    _ = 1.0 / fz\n}\n", "HYB-E6001");
    assert_code("fn main() {\n    _ = float64(1) / 0.0\n}\n", "HYB-E6001");
    assert_code("const num = float64(5)\n\nfn main() {\n    _ = num / 0.0\n}\n", "HYB-E6001");
}

#[test]
fn int_conversion_of_const_len_and_chained_string_consts_reject() {
    assert_code("fn main() {\n    _ = 10 / int(len(\"\"))\n}\n", "HYB-E6001");
    assert_code("const s = \"abc\"\n\nfn main() {\n    _ = 10 % int64(len(s) - 3)\n}\n", "HYB-E6001");
    assert_code("fn main() {\n    _ = 10 / u8(len(\"\") + 256)\n}\n", "HYB-E6001");
    assert_code("const a = \"\"\nconst b = a\n\nfn main() {\n    _ = 10 / len(b)\n}\n", "HYB-E6001");
    assert_code("const a = \"ab\"\nconst b = a + \"c\"\n\nfn main() {\n    _ = 10 / (len(b) - 3)\n}\n", "HYB-E6001");
    assert_code("fn main() {\n    _ = 10 / len(\"\" + \"\")\n}\n", "HYB-E6001");
}

#[test]
fn shadowed_conversions_and_closure_shadowed_dividends_stay_accepted() {
    assert_ok("const zero = 0\n\nfn main() {\n    f := fn(zero int) int {\n        return 10 / int(zero)\n    }\n    _ = f(2)\n}\n");
    assert_ok("const c = 1.0\n\nfn main() {\n    f := fn(c float64) float64 {\n        return c / 0.0\n    }\n    _ = f(5.0)\n}\n");
    assert_ok("fn main() {\n    float64 := fn(x int) float64 { return 2.0 }\n    _ = 1.0 / float64(0)\n}\n");
    assert_ok("fn main() {\n    int := fn(x int) int { return 2 }\n    _ = 10 / int(0)\n}\n");
    assert_ok("const zero = 0\n\nfn main() {\n    outer := fn() int {\n        return 1 + (fn() int {\n            zero := 2\n            return 10 / zero\n        })()\n    }\n    _ = outer()\n}\n");
    assert_ok("const zero = 0\n\nfn main() {\n    outer := fn(zero int) int {\n        inner := fn() int {\n            return 10 / zero\n        }\n        return inner() + (10 / zero)\n    }\n    _ = outer(2)\n}\n");
    assert_ok("const zero = 0\n\nenum opt {\n    some(int)\n    none\n}\n\nfn main() {\n    f := fn(x opt) int {\n        match x {\n            opt.some(zero) => {\n                return 10 / zero\n            }\n            opt.none => {\n                return 0\n            }\n        }\n    }\n    _ = f(opt.some(2))\n}\n");
    assert_ok("const zero = 0\n\nfn fill(p *int) {\n    _ = p\n}\n\nfn main() {\n    f := fn() (zero int) {\n        fill(&zero)\n        return 10 / zero\n    }\n    _ = f()\n}\n");
}

#[test]
fn int_conversion_of_float_const_zero_divisors_reject() {
    assert_code("fn main() {\n    _ = 10 / int(0.0)\n}\n", "HYB-E6001");
    assert_code("const zf = 0.0\n\nfn main() {\n    _ = 10 % int64(zf)\n}\n", "HYB-E6001");
    assert_code("fn main() {\n    _ = 10 / int(1.5 - 1.5)\n}\n", "HYB-E6001");
    assert_code("fn main() {\n    _ = 10 / u8(-0.0)\n}\n", "HYB-E6001");
    assert_code("const iz = int(0.0)\n\nfn main() {\n    _ = 10 / iz\n}\n", "HYB-E6001");
    assert!(codes("fn main() {\n    _ = 10 / u8(256.0)\n}\n").contains(&"HYB-E6001".to_string()));
}
