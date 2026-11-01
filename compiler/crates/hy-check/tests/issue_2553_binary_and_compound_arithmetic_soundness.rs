//! issue_2553_binary_and_compound_arithmetic_soundness.rs — tests for HYB-E1002 rejection
//! of mismatched float64 and typed integer variables in binary arithmetic (`+`, `-`, `*`, `/`),
//! comparisons (`<`, `<=`, `>`, `>=`), and compound assignments (`+=`, `-=`, `*=`, `/=`) (fixes #2553).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn float_and_int_var_binary_arithmetic_rejects_e1002() {
    let cases = [
        "fn main() {\n    var f float64 = 1.5\n    var i int = 2\n    c := f + i\n    _ = c\n}\n",
        "fn main() {\n    var f float64 = 1.5\n    var i int = 2\n    c := f - i\n    _ = c\n}\n",
        "fn main() {\n    var f float64 = 1.5\n    var i int = 2\n    c := f * i\n    _ = c\n}\n",
        "fn main() {\n    var f float64 = 1.5\n    var i int = 2\n    c := f / i\n    _ = c\n}\n",
        "fn main() {\n    var f float64 = 1.5\n    var i int = 2\n    c := i + f\n    _ = c\n}\n",
        "fn main() {\n    var f float64 = 1.5\n    var i int = 2\n    c := i - f\n    _ = c\n}\n",
        "fn main() {\n    var f float64 = 1.5\n    var i int = 2\n    c := i * f\n    _ = c\n}\n",
        "fn main() {\n    var f float64 = 1.5\n    var i int = 2\n    c := i / f\n    _ = c\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:\n{}", src);
        assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    }
}

#[test]
fn float_and_int_var_comparisons_rejects_e1002() {
    let cases = [
        "fn main() {\n    var f float64 = 1.5\n    var i int = 2\n    c := f < i\n    _ = c\n}\n",
        "fn main() {\n    var f float64 = 1.5\n    var i int = 2\n    c := f <= i\n    _ = c\n}\n",
        "fn main() {\n    var f float64 = 1.5\n    var i int = 2\n    c := f > i\n    _ = c\n}\n",
        "fn main() {\n    var f float64 = 1.5\n    var i int = 2\n    c := f >= i\n    _ = c\n}\n",
        "fn main() {\n    var f float64 = 1.5\n    var i int = 2\n    c := i < f\n    _ = c\n}\n",
        "fn main() {\n    var f float64 = 1.5\n    var i int = 2\n    c := i <= f\n    _ = c\n}\n",
        "fn main() {\n    var f float64 = 1.5\n    var i int = 2\n    c := i > f\n    _ = c\n}\n",
        "fn main() {\n    var f float64 = 1.5\n    var i int = 2\n    c := i >= f\n    _ = c\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:\n{}", src);
        assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    }
}

#[test]
fn float_compound_assign_int_var_rejects_e1002() {
    let cases = [
        "fn main() {\n    var f float64 = 1.5\n    var i int = 2\n    f += i\n    _ = f\n}\n",
        "fn main() {\n    var f float64 = 1.5\n    var i int = 2\n    f -= i\n    _ = f\n}\n",
        "fn main() {\n    var f float64 = 1.5\n    var i int = 2\n    f *= i\n    _ = f\n}\n",
        "fn main() {\n    var f float64 = 1.5\n    var i int = 2\n    f /= i\n    _ = f\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:\n{}", src);
        assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    }
}

#[test]
fn valid_untyped_int_constants_accepted() {
    let valid_src = "\
const file_k = 2

fn main() {
    const local_k = 3
    var f float64 = 1.5
    a := f + 1
    b := 1 + f
    c := f - 1
    d := 2 - f
    e := f * 2
    g := 2 * f
    h := f / 2
    j := 2 / f
    k := f < 2
    l := 1 < f
    m := f + file_k
    n := local_k * f
    f += 1
    f -= 1
    f *= 2
    f /= 2
    f += file_k
    f *= local_k
    _ = a
    _ = b
    _ = c
    _ = d
    _ = e
    _ = g
    _ = h
    _ = j
    _ = k
    _ = l
    _ = m
    _ = n
    _ = f
}
";
    let r = check(valid_src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept for untyped int constants, got diags: {:?}",
        r.diagnostics
    );
}
