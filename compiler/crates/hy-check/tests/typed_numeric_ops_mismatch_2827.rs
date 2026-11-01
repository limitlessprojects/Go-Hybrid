//! typed_numeric_ops_mismatch_2827.rs — issue #2827 (Row 13 BUILDABLE).
//!
//! Go §Arithmetic operators: "In every other case, the operands must be of the
//! same type." `OpKind` collapses every integer width into `Int`, so `a64 % b32`,
//! `a64 & b32` and the compound forms (`a64 += b32`, `a64 &= b32`) passed
//! `hyb check` and failed `go build` (`invalid operation: a64 % b32 (mismatched
//! types int64 and int32)`). Shifts are deliberately exempt: Go allows a shift
//! count of any integer type.

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
fn mismatched_typed_numeric_operands_reject() {
    // The issue's own reproducer.
    let r = check(
        "fn main() {\n    var a64 int64 = 10\n    var b32 int32 = 3\n    _ = a64 % b32\n    _ = a64 & b32\n    _ = a64 | b32\n    _ = a64 ^ b32\n    a64 %= b32\n    a64 &= b32\n    a64 |= b32\n    a64 ^= b32\n}\n",
    );
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().filter(|c| *c == "HYB-E1002").count() >= 8,
        "expected at least eight HYB-E1002, got {codes:?}"
    );

    for (name, src) in [
        ("mod", "fn main() {\n    var a64 int64 = 10\n    var b32 int32 = 3\n    _ = a64 % b32\n}\n"),
        ("bitand", "fn main() {\n    var a64 int64 = 10\n    var b32 int32 = 3\n    _ = a64 & b32\n}\n"),
        ("bitor", "fn main() {\n    var a64 int64 = 10\n    var b32 int32 = 3\n    _ = a64 | b32\n}\n"),
        ("bitxor", "fn main() {\n    var a64 int64 = 10\n    var b32 int32 = 3\n    _ = a64 ^ b32\n}\n"),
        ("add-assign", "fn main() {\n    var a64 int64 = 10\n    var b32 int32 = 3\n    a64 += b32\n    _ = a64\n}\n"),
        ("sub-assign", "fn main() {\n    var a64 int64 = 10\n    var b32 int32 = 3\n    a64 -= b32\n    _ = a64\n}\n"),
        ("mul-assign", "fn main() {\n    var a64 int64 = 10\n    var b32 int32 = 3\n    a64 *= b32\n    _ = a64\n}\n"),
        ("div-assign", "fn main() {\n    var a64 int64 = 10\n    var b32 int32 = 3\n    a64 /= b32\n    _ = a64\n}\n"),
        ("mod-assign", "fn main() {\n    var a64 int64 = 10\n    var b32 int32 = 3\n    a64 %= b32\n    _ = a64\n}\n"),
        ("and-assign", "fn main() {\n    var a64 int64 = 10\n    var b32 int32 = 3\n    a64 &= b32\n    _ = a64\n}\n"),
        ("or-assign", "fn main() {\n    var a64 int64 = 10\n    var b32 int32 = 3\n    a64 |= b32\n    _ = a64\n}\n"),
        ("xor-assign", "fn main() {\n    var a64 int64 = 10\n    var b32 int32 = 3\n    a64 ^= b32\n    _ = a64\n}\n"),
        ("float compound", "fn main() {\n    var f64 float64 = 1.0\n    var f32 float32 = 2.0\n    f64 += f32\n    _ = f64\n}\n"),
        ("uint widths", "fn main() {\n    var u64 uint64 = 1\n    var u8 uint8 = 2\n    _ = u64 | u8\n}\n"),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "{name}");
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "{name}: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn untyped_constants_same_widths_shifts_and_aliases_stay_accepted() {
    for (name, src) in [
        (
            "untyped constants adapt on every operator",
            "fn main() {\n    var a64 int64 = 10\n    _ = a64 % 3\n    _ = a64 & 1\n    _ = a64 | 1\n    _ = a64 ^ 1\n    a64 += 1\n    a64 -= 1\n    a64 *= 2\n    a64 /= 2\n    a64 %= 3\n    a64 &= 1\n    a64 |= 1\n    a64 ^= 1\n}\n",
        ),
        (
            "same widths",
            "fn main() {\n    var a64 int64 = 10\n    var b64 int64 = 3\n    _ = a64 % b64\n    _ = a64 & b64\n    a64 += b64\n    a64 &= b64\n}\n",
        ),
        (
            "shift count of another integer type stays legal",
            "fn main() {\n    var a64 int64 = 10\n    var b32 int32 = 3\n    _ = a64 << b32\n    _ = a64 >> b32\n    a64 <<= b32\n    a64 >>= b32\n    _ = a64\n}\n",
        ),
        (
            "byte/uint8 and rune/int32 aliases",
            "fn main() {\n    var b byte = 1\n    var u uint8 = 2\n    _ = b & u\n    b |= u\n    var r rune = 3\n    var i32 int32 = 4\n    _ = r | i32\n}\n",
        ),
        (
            "string += is untouched",
            "fn main() {\n    var s string = \"a\"\n    s += \"b\"\n    _ = s\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "{name}: {:?}", r.diagnostics);
        assert!(error_codes(&r).is_empty(), "{name}: {:?}", r.diagnostics);
    }
}
