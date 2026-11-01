//! typed_conversion_default_int_2698.rs: regression for issue #2698.
//!
//! `x := u64(1 << 63)` declares `x` as `uint64` in Go. The default-`int`
//! range check (HYB-E6002, issue #228) is for UNTYPED constants only, but it
//! used to fold the conversion and compare 2^63 against `int`'s range, so
//! valid programs such as `x := u64(0xcbf29ce484222325)` (the FNV-1a 64-bit
//! offset basis) were rejected. The typed result is still range-checked where
//! Go checks it: HYB-E6006 for the conversion operand and #725 for folded
//! negation / binary / shift results.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn errors(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect()
}

fn program(init: &str) -> String {
    format!("fn main() {{\n    x := {}\n    fmt.println(x)\n}}\n", init)
}

fn accepts(init: &str) {
    let r = check(&program(init));
    let es = errors(&r);
    assert!(es.is_empty(), "`x := {}` must be accepted (Go builds it), got {:?}", init, es);
    assert_ne!(r.verdict, Verdict::Reject, "verdict for `x := {}`", init);
}

fn rejects_e6002(init: &str) {
    let r = check(&program(init));
    let es = errors(&r);
    assert!(
        es.iter().any(|e| e.starts_with("HYB-E6002")),
        "`x := {}` must still reject with HYB-E6002, got {:?}",
        init,
        es
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for `x := {}`", init);
}

#[test]
fn typed_u64_conversion_above_int_max_is_accepted() {
    accepts("u64(1 << 63)");
    accepts("u64(18446744073709551615)");
    accepts("u64(0xFFFFFFFFFFFFFFFF)");
    accepts("u64(0xcbf29ce484222325)");
    accepts("(u64(1 << 63))");
}

#[test]
fn word_sized_conversion_keeps_the_default_int_guard() {
    // The emitter does not fold `uint`/`uintptr`/`int` conversions, and
    // HYB-E6006 only sees literal operands, so this guard must keep stopping
    // a constant expression that would not build (`constant -1 overflows uint`).
    rejects_e6002("uint(0 - 1)");
    rejects_e6002("uintptr(0 - 1)");
}

#[test]
fn typed_operand_propagates_through_binary_and_shift() {
    // Go: the result of `uint64(1) << 63` / `uint64(1 << 62) * 2` is uint64.
    accepts("u64(1) << 63");
    accepts("u64(1 << 62) * 2");
    accepts("u64(18446744073709551615) / 2");
}

#[test]
fn typed_file_level_const_is_not_default_int() {
    let src = "const big u64 = 18446744073709551615\nfn main() {\n    x := big\n    fmt.println(x)\n}\n";
    let r = check(src);
    assert!(errors(&r).is_empty(), "got {:?}", errors(&r));
}

#[test]
fn typed_result_overflow_still_rejects() {
    // #725's checks still own the typed range.
    rejects_e6002("u64(1) << 64");
    rejects_e6002("u64(18446744073709551615) + 1");
    rejects_e6002("u8(255) + 1");
    rejects_e6002("-u64(1)");
}

#[test]
fn untyped_constant_still_gets_default_int() {
    // The #228 rule itself is unchanged for untyped initializers.
    rejects_e6002("1 << 63");
    rejects_e6002("9223372036854775808");
    rejects_e6002("18446744073709551615");
}
