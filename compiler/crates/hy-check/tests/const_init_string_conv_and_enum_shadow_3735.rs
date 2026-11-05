//! Regression tests for #3735:
//! 1. `is_const_string_expr` requires `expr_is_constant_inner` on `string(...)`
//!    conversion arguments (`const n = len("a" + string(x))`) and rejects
//!    conversion targets shadowed in `self.consts` (`const string = 1; const n = len(string("abc"))`).
//! 2. Enum- and type-parameter-shadowed `const` type ascriptions (`enum int { a }; const x int = 1; const y = x`
//!    and `enum string { a }; const s string = "abc"; const n = len(s)`) are rejected with `HYB-E6003`.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_e6003(src: &str, min_count: usize) {
    let r = check(src);
    let count = r.diagnostics.iter().filter(|d| d.code == "HYB-E6003").count();
    assert!(
        count >= min_count,
        "expected at least {} HYB-E6003 for:\n{}\ngot: {:?}",
        min_count,
        src,
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn non_const_string_conversion_args_and_const_shadow_rejected_e6003_3735() {
    assert_e6003(
        "fn main() {\n    x := 65\n    const n = len(\"a\" + string(x))\n    _ = n\n}\n",
        1,
    );
    assert_e6003(
        "fn get_str() string { return \"abc\" }\nconst n = len(\"a\" + string(get_str()))\nfn main() {}\n",
        1,
    );
    assert_e6003(
        "const string = 1\nconst n = len(string(\"abc\"))\nfn main() {}\n",
        1,
    );
}

#[test]
fn enum_and_type_param_shadowed_const_type_ascriptions_rejected_e6003_3735() {
    assert_e6003(
        "enum int { a }\nconst x int = 1\nconst y = x\nfn main() {}\n",
        1,
    );
    assert_e6003(
        "enum string { a }\nconst s string = \"abc\"\nconst n = len(s)\nfn main() {}\n",
        1,
    );
}
