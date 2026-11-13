//! Regression tests for #3708:
//! 1. `const_len_str_value` must consult `st.env_lookup` (for both `"len"` and
//!    the string identifier argument) in addition to `defined`, so a parameter
//!    or local variable shadowing a file-level `const s = ""` does not falsely
//!    trigger `HYB-E6001` on `10 / len(s)`.
//! 2. `divisor_int_fold` must recursively fold `Expr::Unary { op: UnOp::Neg, .. }`
//!    so `10 / -len("")` and `10 / (3 + -len("abc"))` are rejected with
//!    `HYB-E6001`, while a closure parameter shadowing `const zero = 0` inside
//!    `10 / -zero` is not falsely flagged.

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("parses");
    check_file(&ast)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn assert_code(src: &str, want: &str) {
    let cs = codes(src);
    assert!(
        !cs.is_empty() && cs.iter().all(|c| c == want),
        "expected only [{want}], got {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(cs.is_empty(), "expected 0 diagnostics, got {cs:?} for:\n{src}");
}

#[test]
fn negated_const_len_zero_divisor_rejects() {
    assert_code(
        "fn main() {\n    _ = 10 / -len(\"\")\n}\n",
        "HYB-E6001",
    );
    assert_code(
        "const s = \"\"\n\nfn main() {\n    _ = 10 % -len(s)\n}\n",
        "HYB-E6001",
    );
    assert_code(
        "const s = \"abc\"\n\nfn main() {\n    _ = 10 / (3 + -len(s))\n}\n",
        "HYB-E6001",
    );
}

#[test]
fn shadowed_const_string_in_len_divisor_is_allowed() {
    assert_ok(
        "const s = \"\"\n\nfn by_param(s string) int {\n    return 10 / len(s)\n}\n\nfn main() {\n    _ = by_param(\"abc\")\n}\n",
    );
    assert_ok(
        "const s = \"\"\n\nfn main() {\n    s := \"abc\"\n    _ = 10 / len(s)\n}\n",
    );
}

#[test]
fn closure_shadowed_const_in_negated_divisor_is_allowed() {
    assert_ok(
        "const zero = 0\n\nfn main() {\n    f := fn(zero int) int {\n        return 10 / -zero\n    }\n    _ = f(2)\n}\n",
    );
    assert_code(
        "const zero = 0\n\nfn main() {\n    f := fn(other int) int {\n        return other / -zero\n    }\n    _ = f(10)\n}\n",
        "HYB-E6001",
    );
}
