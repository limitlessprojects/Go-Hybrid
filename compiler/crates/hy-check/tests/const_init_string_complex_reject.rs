//! Regression tests (reject) for #2422: run-time `string(int/rune)` conversions
//! and domain-invalid `complex` / `real` / `imag` constant initializers (`HYB-E6003`).

use hy_check::check_file;
use hy_syntax::parse_file;

fn assert_e6003(src: &str) {
    let ast = parse_file("test.hy", src).expect("parses");
    let codes: Vec<String> = check_file(&ast).diagnostics.into_iter().map(|d| d.code).collect();
    assert!(codes.iter().any(|c| c == "HYB-E6003"), "expected HYB-E6003, got {codes:?} for:\n{src}");
}

#[test]
fn string_int_and_rune_conversions_in_const_reject() {
    assert_e6003("const s1 = string(65)\nfn main() {}\n");
    assert_e6003("const s2 = string('a')\nfn main() {}\n");
    assert_e6003("type my_str = string\nconst s3 = my_str(65)\nfn main() {}\n");
    assert_e6003("const n1 = len(string(65))\nfn main() {}\n");
    assert_e6003("fn main() {\n    const local_s = string(65)\n    _ = local_s\n}\n");
}

#[test]
fn complex_real_and_imag_non_numeric_and_nested_complex_operands_reject() {
    assert_e6003("const c1 = complex(\"a\", \"b\")\nfn main() {}\n");
    assert_e6003("const c2 = complex(true, false)\nfn main() {}\n");
    assert_e6003("const c3 = complex(complex(1, 2), 3)\nfn main() {}\n");
    assert_e6003("const r1 = real(\"hello\")\nfn main() {}\n");
    assert_e6003("const i1 = imag(true)\nfn main() {}\n");
    assert_e6003("const flag = 1 < 2\nconst i2 = imag(flag)\nfn main() {}\n");
}
