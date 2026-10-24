//! binop_operand_types_issue_922.rs — regression tests for issue #922:
//! arithmetic (`+ - * /`) and ordered comparisons (`< <= > >=`) between
//! mismatched concrete numeric types must be HYB-E1002, while Go's
//! untyped-constant adaptation keeps every legal spelling building.
//!
//! Root cause: `check_binary_operands` only compared numeric FAMILIES
//! (`lk.is_numeric() != rk.is_numeric()` for `< <= > >=` and `+`; kind
//! membership for `- * /`), so `int` and `float64` — both numeric — never
//! mismatched. The fix routes cross-kind pairs through
//! `Collector::numeric_adapt_conflict`, which allows exactly Go's two
//! adaptation lanes (`1 + 2.5` both-untyped, `f + 1` int-const→float64,
//! including untyped `const` identifiers via `is_untyped_constant` #1112)
//! and rejects `x + f`, `f + x`, `x + 2.5`.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "test_binop_operand_types.hy";
    let ast = parse_file(path, src)
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

fn assert_reject_e1002(src: &str, label: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{label}: must reject");
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E1002"),
        "{label}: expected HYB-E1002, got {:?}",
        error_codes(&r)
    );
}

fn assert_accept_clean(src: &str, label: &str) {
    let r = check(src);
    assert!(
        error_codes(&r).is_empty(),
        "{label}: must pass cleanly, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept, "{label}: must accept");
}

// ---------------------------------------------------------------------------
// Reject: typed int variable vs typed float64 variable, one per operator
// ---------------------------------------------------------------------------

#[test]
fn add_typed_int_and_float_vars_rejected() {
    assert_reject_e1002(
        "
fn main() {
    x := 10
    var f float64 = 2.5
    _ = x + f
    fmt.println(\"{x}{f}\")
}
",
        "`x + f`",
    );
}

#[test]
fn sub_typed_int_and_float_vars_rejected() {
    assert_reject_e1002(
        "
fn main() {
    x := 10
    var f float64 = 2.5
    _ = x - f
    fmt.println(\"{x}{f}\")
}
",
        "`x - f`",
    );
}

#[test]
fn mul_typed_int_and_float_vars_rejected() {
    assert_reject_e1002(
        "
fn main() {
    x := 10
    var f float64 = 2.5
    _ = x * f
    fmt.println(\"{x}{f}\")
}
",
        "`x * f`",
    );
}

#[test]
fn div_typed_int_and_float_vars_rejected() {
    assert_reject_e1002(
        "
fn main() {
    x := 10
    var f float64 = 2.5
    _ = x / f
    fmt.println(\"{x}{f}\")
}
",
        "`x / f`",
    );
}

#[test]
fn lt_typed_int_and_float_vars_rejected() {
    assert_reject_e1002(
        "
fn main() {
    x := 10
    var f float64 = 2.5
    _ = x < f
    fmt.println(\"{x}{f}\")
}
",
        "`x < f`",
    );
}

#[test]
fn le_float_and_int_vars_rejected() {
    assert_reject_e1002(
        "
fn main() {
    x := 10
    var f float64 = 2.5
    _ = f <= x
    fmt.println(\"{x}{f}\")
}
",
        "`f <= x`",
    );
}

#[test]
fn gt_float_and_int_vars_rejected() {
    assert_reject_e1002(
        "
fn main() {
    x := 10
    var f float64 = 2.5
    _ = f > x
    fmt.println(\"{x}{f}\")
}
",
        "`f > x`",
    );
}

#[test]
fn ge_int_and_float_vars_rejected() {
    assert_reject_e1002(
        "
fn main() {
    x := 10
    var f float64 = 2.5
    _ = x >= f
    fmt.println(\"{x}{f}\")
}
",
        "`x >= f`",
    );
}

// ---------------------------------------------------------------------------
// Reject: typed int variable against an untyped FLOAT constant — Go reports
// mismatched types (the const cannot become int), unlike int-const → float64
// ---------------------------------------------------------------------------

#[test]
fn add_int_var_and_float_literal_rejected() {
    assert_reject_e1002(
        "
fn main() {
    x := 10
    _ = x + 2.5
    fmt.println(\"{x}\")
}
",
        "`x + 2.5`",
    );
}

#[test]
fn sub_float_literal_and_int_var_rejected() {
    assert_reject_e1002(
        "
fn main() {
    x := 10
    _ = 2.5 - x
    fmt.println(\"{x}\")
}
",
        "`2.5 - x`",
    );
}

#[test]
fn lt_int_var_and_float_literal_rejected() {
    assert_reject_e1002(
        "
fn main() {
    x := 10
    _ = x < 2.5
    fmt.println(\"{x}\")
}
",
        "`x < 2.5`",
    );
}

// ---------------------------------------------------------------------------
// Reject: reversed direction and typed const — a typed `const k int` is as
// concrete as a variable; a unary minus over a variable is typed too
// ---------------------------------------------------------------------------

#[test]
fn add_float_var_and_int_var_rejected() {
    assert_reject_e1002(
        "
fn main() {
    x := 10
    var f float64 = 2.5
    _ = f + x
    fmt.println(\"{x}{f}\")
}
",
        "`f + x`",
    );
}

#[test]
fn mul_float_var_and_int_literal_expr_rejected() {
    assert_reject_e1002(
        "
fn main() {
    x := 10
    var f float64 = 2.5
    _ = f * x
    fmt.println(\"{x}{f}\")
}
",
        "`f * x`",
    );
}

#[test]
fn add_float_var_and_typed_int_const_rejected() {
    assert_reject_e1002(
        "
fn main() {
    const k int = 1
    var f float64 = 2.5
    _ = f + k
    fmt.println(\"{f}\")
}
",
        "`f + k` with `const k int`",
    );
}

#[test]
fn add_float_var_and_negated_int_var_rejected() {
    assert_reject_e1002(
        "
fn main() {
    x := 10
    var f float64 = 2.5
    _ = f + -x
    fmt.println(\"{x}{f}\")
}
",
        "`f + -x`",
    );
}

// ---------------------------------------------------------------------------
// Accept: untyped-constant adaptation — every spelling Go builds
// ---------------------------------------------------------------------------

#[test]
fn add_float_var_and_int_literal_accepted() {
    assert_accept_clean(
        "
fn main() {
    var f float64 = 2.5
    _ = f + 1
    fmt.println(\"{f}\")
}
",
        "`f + 1`",
    );
}

#[test]
fn add_int_literal_and_float_var_accepted() {
    assert_accept_clean(
        "
fn main() {
    var f float64 = 2.5
    _ = 1 + f
    fmt.println(\"{f}\")
}
",
        "`1 + f`",
    );
}

#[test]
fn add_two_untyped_literals_accepted() {
    assert_accept_clean(
        "
fn main() {
    _ = 1 + 2.5
    fmt.println(\"ok\")
}
",
        "`1 + 2.5`",
    );
}

#[test]
fn add_float_var_and_untyped_const_ident_accepted() {
    assert_accept_clean(
        "
fn main() {
    const k = 1
    var f float64 = 2.5
    _ = f + k
    fmt.println(\"{f}\")
}
",
        "`f + k` with untyped `const k = 1`",
    );
}

#[test]
fn add_two_untyped_const_idents_accepted() {
    assert_accept_clean(
        "
fn main() {
    const a = 1
    const b = 2.5
    _ = a + b
    fmt.println(\"ok\")
}
",
        "`a + b` untyped consts",
    );
}

#[test]
fn add_int_var_and_int_literal_accepted() {
    assert_accept_clean(
        "
fn main() {
    x := 10
    _ = x + 1
    _ = x < 10
    _ = x * 2
    _ = x / 2
    _ = x - 1
    fmt.println(\"{x}\")
}
",
        "int ops with int literals",
    );
}

#[test]
fn same_kind_vars_accepted() {
    assert_accept_clean(
        "
fn main() {
    x := 10
    y := 3
    var a float64 = 1.5
    var b float64 = 2.5
    _ = x + y
    _ = x < y
    _ = a + b
    _ = a >= b
    fmt.println(\"{x}{y}{a}{b}\")
}
",
        "int/int and float64/float64 pairs",
    );
}

#[test]
fn string_concat_and_order_accepted() {
    assert_accept_clean(
        "
fn main() {
    s := \"a\"
    _ = s + \"b\"
    _ = s < \"c\"
    _ = \"x\" + s
    fmt.println(\"{s}\")
}
",
        "string `+` and `<`",
    );
}

#[test]
fn compound_assign_adaptation_accepted() {
    assert_accept_clean(
        "
fn main() {
    var f float64 = 2.5
    f += 1
    f -= 1
    x := 10
    x += 2
    fmt.println(\"{f}{x}\")
}
",
        "`f += 1` / `x += 2`",
    );
}

