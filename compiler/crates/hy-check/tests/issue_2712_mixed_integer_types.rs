//! issue_2712_mixed_integer_types.rs — HYB-E1002 for typed integer operands
//! of different concrete types (`u8 + i8`, `i32 < i64`, `a += b`, `a = b`).
//! All integer types share `OpKind::Int`, so these used to pass `hyb check`
//! and fail `go build` with "mismatched types" (fixes #2712).

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

fn wrap(decls: &str, body: &str) -> String {
    format!("fn main() {{\n{}\n{}\n}}\n", decls, body)
}

fn assert_rejects(src: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:\n{}", src);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for:\n{}\ngot {:?}",
        src,
        error_codes(&r)
    );
}

fn assert_accepts(src: &str) {
    let r = check(src);
    assert!(
        !error_codes(&r).contains(&"HYB-E1002".to_string()),
        "unexpected HYB-E1002 for:\n{}\n{:?}",
        src,
        r.diagnostics
    );
}

#[test]
fn issue_repro_u8_plus_i8_rejects() {
    assert_rejects(&wrap(
        "    var x u8 = 200\n    var y i8 = 5",
        "    fmt.println(x + y)",
    ));
}

#[test]
fn mixed_width_binary_ops_reject() {
    let decls = "    var a i32 = 1\n    var b i64 = 2";
    for e in [
        "a + b", "a - b", "a * b", "a / b", "a % b", "a < b", "a <= b", "a > b", "a >= b",
        "a == b", "a != b", "a & b", "a | b", "a ^ b", "b + a", "(a) + (b)",
    ] {
        assert_rejects(&wrap(decls, &format!("    fmt.println({})", e)));
    }
}

#[test]
fn mixed_width_assignments_reject() {
    let decls = "    var a i32 = 1\n    var b i64 = 2";
    for s in ["a = b", "a += b", "a -= b", "a *= b", "a /= b", "a %= b", "a |= b", "a &= b", "a ^= b"] {
        assert_rejects(&wrap(decls, &format!("    {}\n    fmt.println(a, b)", s)));
    }
}

#[test]
fn signedness_and_conversion_operands_reject() {
    assert_rejects(&wrap("    var a u32 = 1\n    var b i32 = 2", "    fmt.println(a + b)"));
    assert_rejects(&wrap("    var a int = 1\n    var b i64 = 2", "    fmt.println(a + b)"));
    assert_rejects(&wrap("    var a i32 = 1\n    var b i64 = 2", "    fmt.println(i64(a) + i32(b))"));
}

#[test]
fn matching_types_aliases_and_untyped_constants_accept() {
    let ok = [
        // explicit conversion — the fix the diagnostic suggests
        wrap("    var a i32 = 1\n    var b i64 = 2", "    fmt.println(i64(a) + b)\n    a = i32(b)\n    fmt.println(a)"),
        // same type, Go and SPEC spellings, Go aliases
        wrap("    var a i32 = 1\n    var b int32 = 2", "    fmt.println(a + b)"),
        wrap("    var a u8 = 1\n    var b byte = 2", "    fmt.println(a + b, a == b)"),
        wrap("    var a rune = 1\n    var b i32 = 2", "    a += b\n    fmt.println(a)"),
        // untyped constants adapt
        wrap("    var a i32 = 1", "    fmt.println(a + 1, a < 10)\n    a += 3\n    a = 7\n    fmt.println(a)"),
        // shift counts may be any integer type
        wrap("    var a i64 = 1\n    var n u8 = 3", "    fmt.println(a << n, a >> n)\n    a <<= n\n    fmt.println(a)"),
        // ++ / --
        wrap("    var a u16 = 1", "    a++\n    a--\n    fmt.println(a)"),
    ];
    for src in ok {
        assert_accepts(&src);
    }
}

#[test]
fn shadowing_drops_the_outer_integer_type() {
    // inner `b` is an inferred `int`-kind binding with no explicit type:
    // fail-open, the checker does not guess its concrete type.
    assert_accepts(&wrap(
        "    var a i32 = 1\n    var b i64 = 2\n    fmt.println(b)",
        "    if true {\n        b := a\n        fmt.println(a + b)\n    }",
    ));
}
