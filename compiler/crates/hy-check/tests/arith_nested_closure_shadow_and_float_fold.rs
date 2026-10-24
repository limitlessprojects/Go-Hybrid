//! Regression tests for issue #1613: closure-defined shadows of file-level zero
//! constants (`self.zd_defined`) must survive nested closures and apply to unary,
//! conversion, and float constant divisor folds (`HYB-E6001`).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("arith_nested_closure_shadow_and_float_fold.hy", src).expect("parses");
    check_file(&ast)
}

#[test]
fn nested_closure_preserves_outer_closure_shadow_of_zero_const() {
    let r = check(
        r#"const zero = 0
const zf = 0.0

fn main() {
    outer := fn(zero int, zf f64) f64 {
        inner := fn() int {
            return 10 / zero
        }
        sum := inner() + (10 / zero) + (10 / (-zero)) + int(i64(10) / i64(zero))
        return f64(sum) + (1.0 / zf) + (1.0 / f64(zf))
    }
    _ = outer(2, 2.0)
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn closure_inside_binary_expr_records_local_shadow_without_false_e6001() {
    let r = check(
        r#"const zero = 0

fn main() {
    outer := fn() int {
        return (fn() int {
            zero := 2
            return 10 / zero
        }()) + 1
    }
    _ = outer()
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn unshadowed_zero_const_inside_nested_closure_still_rejects_with_e6001() {
    let r_int = check(
        r#"const zero = 0

fn main() {
    outer := fn(other int) int {
        inner := fn() int {
            return 10 / zero
        }
        return inner() + other
    }
    _ = outer(1)
}
"#,
    );
    assert_eq!(r_int.verdict, Verdict::Reject, "{:?}", r_int.diagnostics);
    assert!(
        r_int.diagnostics.iter().any(|d| d.code == "HYB-E6001"),
        "expected HYB-E6001 for unshadowed int zero const in nested closure: {:?}",
        r_int.diagnostics
    );

    let r_float = check(
        r#"const zf = 0.0

fn main() {
    outer := fn(other f64) f64 {
        inner := fn() f64 {
            return 1.0 / zf
        }
        return inner() + other
    }
    _ = outer(1.0)
}
"#,
    );
    assert_eq!(
        r_float.verdict,
        Verdict::Reject,
        "{:?}",
        r_float.diagnostics
    );
    assert!(
        r_float.diagnostics.iter().any(|d| d.code == "HYB-E6001"),
        "expected HYB-E6001 for unshadowed float zero const in nested closure: {:?}",
        r_float.diagnostics
    );
}
