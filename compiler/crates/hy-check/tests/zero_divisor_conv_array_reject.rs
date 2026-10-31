//! Regression tests (reject) for #2426: constant zero divisors across float->int
//! conversions, empty string concatenations, `[0]T` arrays, `real`/`imag`, and `int_var / 0.0` (`HYB-E6001`).

use hy_check::check_file;
use hy_syntax::parse_file;

fn assert_e6001(src: &str) {
    let ast = parse_file("test.hy", src).expect("parses");
    let codes: Vec<String> = check_file(&ast).diagnostics.into_iter().map(|d| d.code).collect();
    assert!(codes.iter().any(|c| c == "HYB-E6001"), "expected HYB-E6001, got {codes:?} for:\n{src}");
}

#[test]
fn int_conversions_of_float_zero_and_type_aliases_reject() {
    assert_e6001("fn main() {\n    x := 10\n    _ = x / int(0.0)\n}\n");
    assert_e6001("const fz = 0.0\ntype my_int = int\nfn main() {\n    x := 10\n    _ = x % my_int(fz)\n}\n");
}

#[test]
fn empty_string_concat_and_zero_length_array_len_cap_reject() {
    assert_e6001("const e = \"\"\nfn main() {\n    x := 10\n    _ = x / len(\"\" + \"\")\n    _ = x / len(e + \"\")\n}\n");
    assert_e6001("fn main() {\n    x := 10\n    _ = x / len([0]int{})\n}\n");
    assert_e6001("fn main() {\n    x := 10\n    var a [0]int\n    _ = x / cap(a)\n}\n");
}

#[test]
fn complex_real_imag_zero_and_int_divided_by_untyped_float_zero_reject() {
    assert_e6001("fn main() {\n    x := 10\n    _ = x / int(real(complex(0, 5)))\n}\n");
    assert_e6001("const c = complex(5.0, 0.0)\nfn main() {\n    _ = 1.0 / imag(c)\n}\n");
    assert_e6001("fn main() {\n    x := 10\n    _ = x / 0.0\n}\n");
    assert_e6001("fn main() {\n    x := 10\n    x /= 0.0\n}\n");
}
