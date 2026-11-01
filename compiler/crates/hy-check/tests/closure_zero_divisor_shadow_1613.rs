#[test]
fn closure_zero_divisor_shadow_issue_1613() {
    // Repro from issue #1613: nested closures and unary/float divisors shadow file-level zero consts.
    let repro = r#"
const zero = 0
const zf = 0.0

fn main() {
    outer := fn(zero int, zf f64) f64 {
        inner := fn() int {
            return 10 / zero
        }
        return f64(inner() + 10 / (-zero) + 10 / zero) + (1.0 / zf)
    }
    _ = outer(2, 2.0)
}
"#;
    let r = hy_check::check_file(&hy_syntax::parse_file("repro_1613.hy", repro).unwrap());
    assert_eq!(r.verdict, hy_check::Verdict::Accept, "expected accept: {:?}", r.diagnostics);

    // Negative control: genuine unshadowed file const zero divisor inside closure is rejected with HYB-E6001
    let neg_int = r#"
const zero = 0

fn main() {
    c := fn() int {
        return 10 / zero
    }
    _ = c()
}
"#;
    let r_neg = hy_check::check_file(&hy_syntax::parse_file("neg_int.hy", neg_int).unwrap());
    assert_eq!(r_neg.verdict, hy_check::Verdict::Reject);
    assert!(r_neg.diagnostics.iter().any(|d| d.code == "HYB-E6001"), "{:?}", r_neg.diagnostics);

    // Negative control: genuine unshadowed float const zero divisor inside closure is rejected with HYB-E6001
    let neg_float = r#"
const zf = 0.0

fn main() {
    c := fn() f64 {
        return 1.0 / zf
    }
    _ = c()
}
"#;
    let r_float = hy_check::check_file(&hy_syntax::parse_file("neg_float.hy", neg_float).unwrap());
    assert_eq!(r_float.verdict, hy_check::Verdict::Reject);
    assert!(r_float.diagnostics.iter().any(|d| d.code == "HYB-E6001"), "{:?}", r_float.diagnostics);
}
