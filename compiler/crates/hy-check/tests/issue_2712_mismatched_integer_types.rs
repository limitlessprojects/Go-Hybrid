//! issue_2712_mismatched_integer_types.rs — tests for HYB-E1002 rejection
//! of mismatched typed integer variables of different widths/signedness (fixes #2712).

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
fn mismatched_integer_binary_arithmetic_rejects_e1002() {
    let cases = [
        "fn main() {\n    var x u8 = 200\n    var y i8 = 5\n    c := x + y\n    _ = c\n}\n",
        "fn main() {\n    var x u8 = 200\n    var y i8 = 5\n    c := x - y\n    _ = c\n}\n",
        "fn main() {\n    var x u8 = 200\n    var y i8 = 5\n    c := x * y\n    _ = c\n}\n",
        "fn main() {\n    var x u8 = 200\n    var y i8 = 5\n    c := x / y\n    _ = c\n}\n",
        "fn main() {\n    var x u8 = 200\n    var y i8 = 5\n    c := x % y\n    _ = c\n}\n",
        "fn main() {\n    var a i32 = 1\n    var b i64 = 2\n    c := a + b\n    _ = c\n}\n",
        "fn main() {\n    var a i32 = 1\n    var b i64 = 2\n    c := a - b\n    _ = c\n}\n",
        "fn main() {\n    var a i32 = 1\n    var b i64 = 2\n    c := a * b\n    _ = c\n}\n",
        "fn main() {\n    var a i32 = 1\n    var b i64 = 2\n    c := a / b\n    _ = c\n}\n",
        "fn main() {\n    var a i32 = 1\n    var b i64 = 2\n    c := a & b\n    _ = c\n}\n",
        "fn main() {\n    var a i32 = 1\n    var b i64 = 2\n    c := a | b\n    _ = c\n}\n",
        "fn main() {\n    var a i32 = 1\n    var b i64 = 2\n    c := a ^ b\n    _ = c\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:\n{}", src);
        assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    }
}

#[test]
fn mismatched_integer_comparisons_rejects_e1002() {
    let cases = [
        "fn main() {\n    var a i32 = 1\n    var b i64 = 2\n    c := a < b\n    _ = c\n}\n",
        "fn main() {\n    var a i32 = 1\n    var b i64 = 2\n    c := a <= b\n    _ = c\n}\n",
        "fn main() {\n    var a i32 = 1\n    var b i64 = 2\n    c := a > b\n    _ = c\n}\n",
        "fn main() {\n    var a i32 = 1\n    var b i64 = 2\n    c := a >= b\n    _ = c\n}\n",
        "fn main() {\n    var a i32 = 1\n    var b i64 = 2\n    c := a == b\n    _ = c\n}\n",
        "fn main() {\n    var a i32 = 1\n    var b i64 = 2\n    c := a != b\n    _ = c\n}\n",
        "fn main() {\n    var x u8 = 200\n    var y i8 = 5\n    c := x == y\n    _ = c\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:\n{}", src);
        assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    }
}

#[test]
fn mismatched_integer_assignments_rejects_e1002() {
    let cases = [
        "fn main() {\n    var a i32 = 1\n    var b i64 = 2\n    a = b\n    _ = a\n}\n",
        "fn main() {\n    var a i32 = 1\n    var b i64 = 2\n    a += b\n    _ = a\n}\n",
        "fn main() {\n    var a i32 = 1\n    var b i64 = 2\n    a -= b\n    _ = a\n}\n",
        "fn main() {\n    var a i32 = 1\n    var b i64 = 2\n    var c i32 = b\n    _ = c\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:\n{}", src);
        assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    }
}

#[test]
fn matching_and_untyped_integer_consts_accepted() {
    let cases = [
        "fn main() {\n    var a i32 = 1\n    c := a + 1\n    _ = c\n}\n",
        "fn main() {\n    var a i32 = 1\n    a += 5\n    _ = a\n}\n",
        "fn main() {\n    var a i32 = 1\n    a = 10\n    _ = a\n}\n",
        "fn main() {\n    var x byte = 1\n    var y uint8 = 2\n    c := x + y\n    _ = c\n}\n",
        "fn main() {\n    var x rune = 1\n    var y int32 = 2\n    c := x + y\n    _ = c\n}\n",
        "fn main() {\n    var a i32 = 1\n    var b i64 = 2\n    c := i64(a) + b\n    _ = c\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "expected accept for:\n{}", src);
        assert_eq!(error_codes(&r).len(), 0);
    }
}
