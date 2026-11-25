//! Regression tests for #3735:
//! 1. `is_const_string_expr` must reject non-constant arguments in `string(...)` conversions
//!    (`const n = len(string(x))`, `const n = len(string(get_str()))`, `const n = len(a + string(x))`)
//!    and targets shadowed in `self.consts` (`const string = 1; const n = len(string("abc"))`).
//! 2. `is_const_allowed_ty` and `is_const_string_expr` must reject `const` type ascriptions
//!    whose resolved type name is shadowed by `structs`, `struct_type_params`, or `enums`
//!    (`enum int { a }; const x int = 1; const y = x`, `enum string { a }; const s string = "abc"; const n = len(s)`).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).expect("must parse");
    check_file(&ast)
}

fn assert_reject_e6003(src: &str) {
    let r = check(src);
    let cs: Vec<_> = r
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.as_str())
        .collect();
    assert!(
        cs.iter().any(|c| *c == "HYB-E6003"),
        "expected HYB-E6003 in {cs:?} for:\n{src}"
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

fn assert_accept(src: &str) {
    let r = check(src);
    let cs: Vec<_> = r
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.as_str())
        .collect();
    assert!(cs.is_empty(), "expected 0 errors, got {cs:?} for:\n{src}");
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn const_init_string_conv_runtime_arg_rejects_e6003() {
    // 1. Runtime variable in string(...) conversion
    assert_reject_e6003(
        "fn main() {\n    x := \"hello\"\n    const n = len(string(x))\n    _ = x\n    _ = n\n}\n",
    );
    // 2. Function call in string(...) conversion
    assert_reject_e6003(
        "fn get_str() string { return \"abc\" }\nfn main() {\n    const n = len(string(get_str()))\n    _ = n\n}\n",
    );
    // 3. Binary concatenation with runtime string(...) conversion
    assert_reject_e6003(
        "fn main() {\n    x := \"world\"\n    const n = len(\"hello \" + string(x))\n    _ = x\n    _ = n\n}\n",
    );
    // 4. const string = 1 shadowing conversion target
    assert_reject_e6003(
        "const string = 1\nconst n = len(string(\"abc\"))\nfn main() {\n    _ = n\n}\n",
    );
}

#[test]
fn const_init_shadowed_type_ascription_rejects_e6003() {
    // 5. enum shadowing int primitive
    assert_reject_e6003(
        "enum int { a }\nconst x int = 1\nconst y = x\nfn main() {\n    _ = y\n}\n",
    );
    // 6. enum shadowing string primitive
    assert_reject_e6003(
        "enum string { a }\nconst s string = \"abc\"\nconst n = len(s)\nfn main() {\n    _ = n\n}\n",
    );
}

#[test]
fn const_init_valid_string_conversions_and_ascriptions_accept() {
    assert_accept(
        "const s = string(\"abc\")\nconst n = len(s)\nfn main() {\n    _ = n\n}\n",
    );
    assert_accept(
        "const a = \"hello \"\nconst b = \"world\"\nconst n = len(a + b)\nfn main() {\n    _ = n\n}\n",
    );
    assert_accept(
        "const x int = 1\nconst y = x + 2\nfn main() {\n    _ = y\n}\n",
    );
}
