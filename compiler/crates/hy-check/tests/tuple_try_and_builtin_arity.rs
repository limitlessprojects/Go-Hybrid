//! Regression tests for `tuple.rs` (`HYB-E2002` / `HYB-E1002` T23 arity checks):
//! 1. `?`-unwrapped calls (`Expr::Try`):
//!    - A `(T1, T2, error)` call unwrapped with `?` yields 2 values; binding 1
//!      or 3+ names or using `f()?` in a single-value expression is rejected
//!      with `HYB-E2002`.
//!    - A `(T, error)` call unwrapped with `?` yields 1 value; binding 2+ names
//!      (`a, b := f()?`) is rejected with `HYB-E1002`.
//! 2. Unshadowed 1-value builtins (`len`, `cap`, `make`, `new`, `append`, `copy`,
//!    `min`, `max`, `real`, `imag`, `complex`, `recover`), prelude constructors
//!    (`some`, `ok`, `err`), and primitive/slice/alias type conversions bound
//!    to 2+ targets (`a, b := len(xs)`, `a, b := some(1)`, `a, b := int64(x)`)
//!    are rejected with `HYB-E1002`.
//! 3. Methods on pointers to type-aliased structs (`type box_alias = pair_box`,
//!    `b *box_alias`, `x := b.unpack()`) resolve the underlying struct method
//!    and enforce `HYB-E2002`.

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

#[test]
fn rejects_try_unwrapped_multi_value_and_single_value_arity_mismatches() {
    // 1. 3-result (int, string, error) call unwrapped with `?` yields 2 values;
    //    binding 1 name (`p := pair_or_err()?`) must emit HYB-E2002.
    let r1 = check(
        r#"
fn pair_or_err() (int, string, error) {
    return 1, "ok", nil
}

fn run() (int, error) {
    p := pair_or_err()?
    return p, nil
}

fn main() {}
"#,
    );
    assert!(
        error_codes(&r1).contains(&"HYB-E2002".to_string()),
        "expected HYB-E2002 for `p := pair_or_err()?`, got: {:?}",
        r1.diagnostics
    );

    // 2. 3-result (int, string, error) call unwrapped with `?` used in a
    //    single-value binary expression (`pair_or_err()? + 1`) must emit HYB-E2002.
    let r2 = check(
        r#"
fn pair_or_err() (int, int, error) {
    return 1, 2, nil
}

fn run() (int, error) {
    x := pair_or_err()? + 1
    return x, nil
}

fn main() {}
"#,
    );
    assert!(
        error_codes(&r2).contains(&"HYB-E2002".to_string()),
        "expected HYB-E2002 for `pair_or_err()? + 1`, got: {:?}",
        r2.diagnostics
    );

    // 3. 2-result (int, error) call unwrapped with `?` yields 1 value;
    //    binding 2 names (`a, b := one_or_err()?`) must emit HYB-E1002.
    let r3 = check(
        r#"
fn one_or_err() (int, error) {
    return 7, nil
}

fn run() (int, error) {
    a, b := one_or_err()?
    return a + b, nil
}

fn main() {}
"#,
    );
    assert!(
        error_codes(&r3).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for `a, b := one_or_err()?`, got: {:?}",
        r3.diagnostics
    );
}

#[test]
fn rejects_single_value_builtins_prelude_and_conversions_bound_to_multiple_names() {
    let cases = [
        "xs := [1, 2, 3]\n    a, b := len(xs)\n    _ = a\n    _ = b",
        "xs := [1, 2]\n    a, b := append(xs, 3)\n    _ = a\n    _ = b",
        "a, b := some(42)\n    _ = a\n    _ = b",
        "x := 7\n    a, b := int64(x)\n    _ = a\n    _ = b",
        "s := \"hi\"\n    a, b := ([]byte)(s)\n    _ = a\n    _ = b",
    ];
    for body in cases {
        let src = format!("fn main() {{\n    {body}\n}}\n");
        let r = check(&src);
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for `{body}`, got: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn rejects_method_on_pointer_to_type_aliased_struct_and_double_pointer_index() {
    let r1 = check(
        r#"
struct pair_box {
    a int
    b int
}

type box_alias = pair_box

fn (p *pair_box) unpack() (int, int) {
    return p.a, p.b
}

fn inspect(b *box_alias) int {
    x := b.unpack()
    return x
}

fn main() {}
"#,
    );
    assert!(
        error_codes(&r1).contains(&"HYB-E2002".to_string()),
        "expected HYB-E2002 for `x := b.unpack()` on `*box_alias`, got: {:?}",
        r1.diagnostics
    );

    let r2 = check(
        r#"
fn inspect(pp **[2]int) int {
    a, b := pp[0]
    return a + b
}

fn main() {}
"#,
    );
    assert!(
        error_codes(&r2).contains(&"HYB-E2002".to_string()),
        "expected HYB-E2002 for `a, b := pp[0]` on `**[2]int`, got: {:?}",
        r2.diagnostics
    );
}

#[test]
fn accepts_valid_try_destructurings_and_single_value_builtins() {
    let r = check(
        r#"
fn pair_or_err() (int, int, error) {
    return 3, 4, nil
}

fn one_or_err() (int, error) {
    return 5, nil
}

fn run() (int, error) {
    a, b := pair_or_err()?
    c := one_or_err()?
    xs := [a, b, c]
    n := len(xs)
    return n, nil
}

fn main() {}
"#,
    );
    assert!(
        error_codes(&r).is_empty(),
        "expected clean accept, got: {:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Accept);
}
