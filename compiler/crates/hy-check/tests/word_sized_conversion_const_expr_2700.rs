//! word_sized_conversion_const_expr_2700.rs: regression for issue #2700.
//!
//! `int(1 << 63)` / `uint(1 << 64)` used to pass `hyb check` with 0
//! diagnostics, and the emitted Go failed `go build` with `constant
//! 9223372036854775808 overflows int`. HYB-E6006 only range-checked literal
//! operands (`int(9223372036854775808)` was already rejected), and the
//! emitter's X4 fold only rewrites the fixed widths (`u8`…`u64`,
//! `i8`…`i64`), never `int`/`uint`/`uintptr`.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn in_main(stmt: &str) -> String {
    format!("fn main() {{\n    {}\n}}\n", stmt)
}

fn rejects_e6006(stmt: &str) {
    let r = check(&in_main(stmt));
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6006"),
        "expected HYB-E6006 for `{}`, got {:?}",
        stmt,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for `{}`", stmt);
}

fn no_e6006(stmt: &str) {
    let r = check(&in_main(stmt));
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E6006"),
        "must NOT fire HYB-E6006 for `{}`, got {:?}",
        stmt,
        cs
    );
}

#[test]
fn int_conversion_of_overflowing_const_expr_rejects() {
    rejects_e6006("x := int(1 << 63)\n    fmt.println(x)");
    rejects_e6006("x := int(9223372036854775807 + 1)\n    fmt.println(x)");
    rejects_e6006("x := int(-(1 << 63) - 1)\n    fmt.println(x)");
    rejects_e6006("x := int(1 << 200)\n    fmt.println(x)");
}

#[test]
fn uint_and_uintptr_conversion_of_overflowing_const_expr_rejects() {
    rejects_e6006("x := uint(1 << 64)\n    fmt.println(x)");
    rejects_e6006("x := uint(0 - 1)\n    fmt.println(x)");
    rejects_e6006("x := uintptr(0 - 1)\n    fmt.println(x)");
}

#[test]
fn overflowing_conversion_outside_a_define_rejects() {
    // No default-int guard applies here at all.
    rejects_e6006("fmt.println(int(1 << 63))");
    rejects_e6006("var y u64 = 0\n    y = u64(int(1 << 63))\n    fmt.println(y)");
}

#[test]
fn fitting_const_expr_conversions_are_accepted() {
    no_e6006("x := int(1 << 62)\n    fmt.println(x)");
    no_e6006("x := int(-(1 << 63))\n    fmt.println(x)");
    no_e6006("x := uint(1 << 63)\n    fmt.println(x)");
    no_e6006("x := int(9223372036854775806 + 1)\n    fmt.println(x)");
}

#[test]
fn fixed_width_targets_keep_the_emitter_fold() {
    // SPEC X4: the emitter folds these, so they stay accepted.
    no_e6006("x := u64(1 << 64)\n    fmt.println(x)");
    no_e6006("x := i64(1 << 63)\n    fmt.println(x)");
    no_e6006("x := u8(1 << 9)\n    fmt.println(x)");
}

#[test]
fn run_time_operands_are_untouched() {
    no_e6006("n := 63\n    x := int(1 << n)\n    fmt.println(x)");
    no_e6006("var v u64 = 18446744073709551615\n    x := int(v)\n    fmt.println(x)");
}
