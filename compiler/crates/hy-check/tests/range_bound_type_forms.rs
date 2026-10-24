//! #827 — non-integer, const, mismatched, and overflowing range bounds.
//!
//! The emitter lowers `for i in lo..hi` to `for i := lo; i < hi; i++`
//! (and `lo..=hi` to `hyLo := lo; hyHi := hi; i, hyCont := hyLo,
//! hyLo <= hyHi; ...`), so Go rejects: non-integer bounds (`0..nil`,
//! `0..(1 < 2)`, `(!b)..5`, `0..(&x)`), untyped const float/bool/string
//! bounds (`0..f` on `const f = 2.5`), bounds resolving to different
//! fixed-width integer types (`0..n` on `var n u32`, `lo i32..hi i64`,
//! `lo i64..=10`), and constant bounds that overflow the counter's type
//! (`0..(1 << 63)`, `lo i8..300`). Untyped literals and untyped consts
//! adapt to the counter and stay legal.

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("parses");
    check_file(&ast)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn assert_code(src: &str, want: &str) {
    let cs = codes(src);
    assert!(
        !cs.is_empty() && cs.iter().all(|c| c == want),
        "expected only [{want}], got {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(cs.is_empty(), "expected 0 diagnostics, got {cs:?} for:\n{src}");
}

#[test]
fn nil_and_bool_result_bounds_reject() {
    assert_code(
        "fn main() {\n    for i in 0..nil { _ = i }\n}\n",
        "HYB-E2009",
    );
    assert_code(
        "fn main() {\n    for i in 0..(1 < 2) { _ = i }\n}\n",
        "HYB-E2009",
    );
    assert_code(
        "fn main() {\n    b := true\n    for i in (!b)..5 { _ = i }\n}\n",
        "HYB-E2009",
    );
}

#[test]
fn untyped_const_non_integer_bounds_reject() {
    assert_code(
        "const f = 2.5\n\nfn main() {\n    for i in 0..f { _ = i }\n}\n",
        "HYB-E2009",
    );
    assert_code(
        "const b = true\n\nfn main() {\n    for i in 0..b { _ = i }\n}\n",
        "HYB-E2009",
    );
}

#[test]
fn mismatched_integer_bound_types_reject() {
    assert_code(
        "fn main() {\n    var n u32 = 5\n    for i in 0..n { _ = i }\n}\n",
        "HYB-E2009",
    );
    assert_code(
        "fn main() {\n    var lo i32 = 0\n    var hi i64 = 5\n    for i in lo..hi { _ = i }\n}\n",
        "HYB-E2009",
    );
    assert_code(
        "fn main() {\n    var lo i64 = 0\n    for i in lo..=10 { _ = i }\n}\n",
        "HYB-E2009",
    );
}

#[test]
fn overflowing_constant_bounds_reject() {
    assert_code(
        "fn main() {\n    for i in 0..(1 << 63) { _ = i }\n}\n",
        "HYB-E2009",
    );
    assert_code(
        "fn main() {\n    for i in 0..99999999999999999999 { _ = i }\n}\n",
        "HYB-E2009",
    );
    assert_code(
        "fn main() {\n    var lo i8 = 0\n    for i in lo..300 { _ = i }\n}\n",
        "HYB-E2009",
    );
}

#[test]
fn valid_integer_ranges_stay_legal() {
    // typed lo + untyped hi: the constant adapts (issue accept control)
    assert_ok(
        "fn main() {\n    var lo i64 = 0\n    for i in lo..10 { _ = i }\n}\n",
    );
    // conversion lo matching the typed hi (issue accept control)
    assert_ok(
        "fn main() {\n    var n u32 = 5\n    for i in u32(0)..n { _ = i }\n}\n",
    );
    // plain literal ranges
    assert_ok("fn main() {\n    for i in 0..10 { _ = i }\n    for i in 0..=8 { _ = i }\n}\n");
    // len() is int
    assert_ok(
        "fn main() {\n    xs := []int{1, 2}\n    for i in 0..len(xs) { _ = i }\n}\n",
    );
    // untyped const bounds adapt
    assert_ok(
        "const n = 5\n\nfn main() {\n    for i in 0..n { _ = i }\n}\n",
    );
    // same-type typed bounds
    assert_ok(
        "fn main() {\n    var lo i64 = 0\n    var hi i64 = 5\n    for i in lo..hi { _ = i }\n}\n",
    );
    // runtime int bound
    assert_ok(
        "fn main() {\n    var n int = 5\n    for i in 0..n { _ = i }\n}\n",
    );
}

#[test]
fn equivalent_go_integer_type_spellings_are_accepted_issue_1165() {
    // u8 / uint8 / byte aliases
    assert_ok("fn main() {\n    var a u8 = 0\n    var b uint8 = 10\n    for i in a..b { _ = i }\n}\n");
    assert_ok("fn main() {\n    var a byte = 0\n    var b uint8 = 10\n    for i in a..b { _ = i }\n}\n");
    assert_ok("fn main() {\n    var a u8 = 0\n    var b byte = 10\n    for i in a..b { _ = i }\n}\n");

    // i32 / int32 / rune aliases
    assert_ok("fn main() {\n    var a i32 = 0\n    var b int32 = 10\n    for i in a..b { _ = i }\n}\n");
    assert_ok("fn main() {\n    var a rune = 0\n    var b int32 = 10\n    for i in a..b { _ = i }\n}\n");
    assert_ok("fn main() {\n    var a i32 = 0\n    var b rune = 10\n    for i in a..b { _ = i }\n}\n");

    // other SPEC / Go integer aliases
    assert_ok("fn main() {\n    var a i8 = 0\n    var b int8 = 10\n    for i in a..b { _ = i }\n}\n");
    assert_ok("fn main() {\n    var a u16 = 0\n    var b uint16 = 10\n    for i in a..b { _ = i }\n}\n");
    assert_ok("fn main() {\n    var a i16 = 0\n    var b int16 = 10\n    for i in a..b { _ = i }\n}\n");
    assert_ok("fn main() {\n    var a u32 = 0\n    var b uint32 = 10\n    for i in a..b { _ = i }\n}\n");
    assert_ok("fn main() {\n    var a u64 = 0\n    var b uint64 = 10\n    for i in a..b { _ = i }\n}\n");
    assert_ok("fn main() {\n    var a i64 = 0\n    var b int64 = 10\n    for i in a..b { _ = i }\n}\n");
}
