//! Integration tests for issue #586 & #576:
//! - Function-local `const` folding across HYB-E6002 (variable declarations,
//!   assignments, return statements, call arguments, struct/slice/array/map
//!   literals, channel sends, untyped default-`int` declarations, binary/comparison
//!   operands), HYB-E6005 (negative shift counts), HYB-E6001 (zero divisors),
//!   HYB-E2005 (`make` negative length / inverted `len > cap`), and HYB-E2009
//!   (range bounds).
//! - Unary negation (`-x`) fixed-width integer propagation in binary/comparison
//!   constant overflow checks (`(-x) == 300`, `-x + 300`).

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet must parse");
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn local_const_overflow_in_declarations_assignments_and_sinks_rejected_e6002() {
    let cases = [
        "fn main() {\n    const big = 300\n    var x int8 = big\n    _ = x\n}\n",
        "fn main() {\n    const a = 100\n    const b = a + 200\n    var x int8 = b\n    _ = x\n}\n",
        "fn main() {\n    const big = 300\n    var x int8 = 1\n    x = big\n    _ = x\n}\n",
        "fn bad_ret() int8 {\n    const k = 300\n    return k\n}\nfn main() {}\n",
        "fn take_i8(v int8) {}\nfn main() {\n    const k = 300\n    take_i8(k)\n}\n",
        "struct box_i8 {\n    v int8\n}\nfn main() {\n    const k = 300\n    b := box_i8{v: k}\n    _ = b\n}\n",
        "fn main() {\n    const k = 300\n    s := []int8{k}\n    _ = s\n}\n",
        "fn main() {\n    const k = 300\n    a := [1]int8{k}\n    _ = a\n}\n",
        "fn main() {\n    const k = 300\n    m := map[int8]int8{k: 1}\n    _ = m\n}\n",
        "fn main() {\n    const k = 300\n    m := map[int8]int8{1: k}\n    _ = m\n}\n",
        "fn send(ch chan int8) {\n    const k = 300\n    ch <- k\n}\nfn main() {}\n",
        "fn main() {\n    const huge = 1 << 100\n    n := huge\n    _ = n\n}\n",
        "fn main() {\n    const k = 300\n    var x int8 = 1\n    b := x == k\n    _ = b\n}\n",
    ];
    for src in cases {
        let r = check(src);
        let cs = error_codes(&r);
        assert!(
            cs.iter().any(|c| c == "HYB-E6002"),
            "expected HYB-E6002 for:\n{src}\ngot: {:?}",
            cs
        );
        assert_eq!(r.verdict, Verdict::Reject);
    }
}

#[test]
fn unary_negation_fixed_width_binary_and_comparison_overflow_rejected_e6002() {
    for src in [
        "fn check_eq(x int8) bool {\n    return (-x) == 300\n}\nfn main() {}\n",
        "fn main() {\n    var x int8 = 1\n    y := -x + 300\n    _ = y\n}\n",
        "fn main() {\n    const k = 300\n    var x int8 = 1\n    b := (-x) < k\n    _ = b\n}\n",
    ] {
        let r = check(src);
        let cs = error_codes(&r);
        assert!(
            cs.iter().any(|c| c == "HYB-E6002"),
            "expected HYB-E6002 for:\n{src}\ngot: {:?}",
            cs
        );
        assert_eq!(r.verdict, Verdict::Reject);
    }
}

#[test]
fn named_and_local_const_negative_shift_zero_divisor_and_make_bounds_rejected() {
    let cases = [
        ("const neg = -1\nfn main() {\n    x := 1 << neg\n    _ = x\n}\n", "HYB-E6005"),
        ("fn main() {\n    const neg = -1\n    x := 1 << neg\n    _ = x\n}\n", "HYB-E6005"),
        ("const neg = -1\nfn main() {\n    x := 1\n    x <<= neg\n    _ = x\n}\n", "HYB-E6005"),
        ("fn main() {\n    const z = 0\n    x := 10\n    y := x / z\n    _ = y\n}\n", "HYB-E6001"),
        ("fn main() {\n    const z = 1 - 1\n    x := 10\n    x %= z\n    _ = x\n}\n", "HYB-E6001"),
        ("fn main() {\n    const neg = -1\n    c := make([]int, neg)\n    _ = c\n}\n", "HYB-E2005"),
        ("fn main() {\n    const l_val = 4\n    const c_val = 2\n    d := make([]int, l_val, c_val)\n    _ = d\n}\n", "HYB-E2005"),
    ];
    for (src, expected) in cases {
        let r = check(src);
        let cs = error_codes(&r);
        assert!(
            cs.iter().any(|c| c == expected),
            "expected {expected} for:\n{src}\ngot: {:?}",
            cs
        );
        assert_eq!(r.verdict, Verdict::Reject);
    }
}

#[test]
fn shadowed_and_in_range_local_consts_accept() {
    for src in [
        "const k = 300\nfn ok(k int8) int8 {\n    return k\n}\nfn main() {}\n",
        "fn main() {\n    const k = 300\n    _ = k\n    if true {\n        var k int8 = 1\n        var y int8 = k\n        _ = y\n    }\n}\n",
        "fn main() {\n    const k = 100\n    var x int8 = k\n    _ = x\n}\n",
        "fn main() {\n    var n int8 = -128\n    var m int8 = -n\n    _ = m\n}\n",
    ] {
        let r = check(src);
        assert!(
            error_codes(&r).is_empty(),
            "expected clean accept for:\n{src}\ngot: {:?}",
            r.diagnostics
        );
        assert_eq!(r.verdict, Verdict::Accept);
    }
}
