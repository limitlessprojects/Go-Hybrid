//! Regression tests for `fold_const_key` in `hy-check/src/emit_guard.rs`:
//! 1. SPEC X4 truncating fixed-width integer conversions (`u8(300)` -> `44`,
//!    `i8(200)` -> `-56`, etc.) in map literal keys must wrap to the target
//!    width before duplicate-key comparison (matching `hy-emit/src/constconv.rs`),
//!    both rejecting true collisions (`u8(300)` vs `44`) with `HYB-E1002` and
//!    accepting distinct post-truncation keys (`int(u8(300))` vs `300`).
//! 2. Signed zero float constant keys (`0.0` vs `-0.0`), float constant binary
//!    arithmetic (`+`, `-`, `*`, `/`), and float conversions (`f32`/`float32`/
//!    `f64`/`float64`) must fold and canonicalize `-0.0` so duplicate float keys
//!    are rejected with `HYB-E1002`.
//! 3. `len("<const string>")` in map literal keys must fold to the string byte
//!    length so `map[int]string{len("abc"): "a", 3: "b"}` is rejected with
//!    `HYB-E1002`.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn assert_rejects_dup_key(r: &hy_check::CheckResult, what: &str) {
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for {what}, got {:?}", r.diagnostics);
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error
                && d.code == "HYB-E1002"
                && d.message.contains("duplicate key")),
        "expected HYB-E1002 duplicate key for {what}, got {:?}",
        r.diagnostics
    );
}

fn assert_accepts(r: &hy_check::CheckResult, what: &str) {
    let errors: Vec<_> = r
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "expected Accept for {what}, got errors: {errors:?}"
    );
    assert_eq!(r.verdict, Verdict::Accept, "{what}");
}

#[test]
fn reject_truncated_u8_and_i8_conversion_duplicate_keys() {
    let u8_src = r#"
fn main() {
    m := map[u8]int{u8(300): 1, 44: 2}
    fmt.println("{len(m)}")
}
"#;
    assert_rejects_dup_key(&check(u8_src), "u8(300) truncates to 44");

    let paren_u8_src = r#"
fn main() {
    m := map[u8]int{(u8)(300): 1, 44: 2}
    fmt.println("{len(m)}")
}
"#;
    assert_rejects_dup_key(&check(paren_u8_src), "(u8)(300) truncates to 44");

    let i8_src = r#"
fn main() {
    m := map[i8]int{i8(200): 1, -56: 2}
    fmt.println("{len(m)}")
}
"#;
    assert_rejects_dup_key(&check(i8_src), "i8(200) truncates to -56");
}

#[test]
fn reject_truncated_u16_i16_u32_i32_conversion_duplicate_keys() {
    let u16_src = r#"
fn main() {
    m := map[u16]int{u16(65537): 1, 1: 2}
    fmt.println("{len(m)}")
}
"#;
    assert_rejects_dup_key(&check(u16_src), "u16(65537) truncates to 1");

    let i16_src = r#"
fn main() {
    m := map[i16]int{i16(40000): 1, -25536: 2}
    fmt.println("{len(m)}")
}
"#;
    assert_rejects_dup_key(&check(i16_src), "i16(40000) truncates to -25536");

    let u32_src = r#"
fn main() {
    m := map[u32]int{u32(4294967297): 1, 1: 2}
    fmt.println("{len(m)}")
}
"#;
    assert_rejects_dup_key(&check(u32_src), "u32(4294967297) truncates to 1");

    let i32_src = r#"
fn main() {
    m := map[i32]int{i32(3000000000): 1, -1294967296: 2}
    fmt.println("{len(m)}")
}
"#;
    assert_rejects_dup_key(&check(i32_src), "i32(3000000000) truncates to -1294967296");
}

#[test]
fn accept_truncated_conversion_distinct_from_untruncated_literal() {
    let src = r#"
fn main() {
    m := map[int]int{int(u8(300)): 1, 300: 2}
    fmt.println("{len(m)}")
}
"#;
    assert_accepts(&check(src), "int(u8(300)) is 44, distinct from 300");
}

#[test]
fn reject_signed_zero_and_folded_float_duplicate_keys() {
    let signed_zero = r#"
fn main() {
    m := map[float64]int{0.0: 1, -0.0: 2}
    fmt.println("{len(m)}")
}
"#;
    assert_rejects_dup_key(&check(signed_zero), "0.0 and -0.0 are duplicate float keys");

    let arith_float = r#"
fn main() {
    m := map[float64]int{1.0 + 0.5: 1, 3.0 / 2.0: 2}
    fmt.println("{len(m)}")
}
"#;
    assert_rejects_dup_key(&check(arith_float), "1.0 + 0.5 and 3.0 / 2.0 both fold to 1.5");

    let conv_float = r#"
fn main() {
    m := map[float64]int{float64(1.5): 1, 1.5: 2}
    fmt.println("{len(m)}")
}
"#;
    assert_rejects_dup_key(&check(conv_float), "float64(1.5) and 1.5 are duplicate keys");

    let int_to_float = r#"
fn main() {
    m := map[float64]int{f64(2): 1, 2.0: 2}
    fmt.println("{len(m)}")
}
"#;
    assert_rejects_dup_key(&check(int_to_float), "f64(2) and 2.0 are duplicate keys");
}

#[test]
fn reject_const_string_len_duplicate_keys_and_accept_distinct() {
    let dup_len = r#"
fn main() {
    m := map[int]string{len("abc"): "a", 3: "b"}
    fmt.println("{len(m)}")
}
"#;
    assert_rejects_dup_key(&check(dup_len), "len(\"abc\") and 3 are duplicate keys");

    let distinct_len = r#"
fn main() {
    m := map[int]string{len("ab"): "a", 3: "b"}
    fmt.println("{len(m)}")
}
"#;
    assert_accepts(&check(distinct_len), "len(\"ab\") (2) and 3 are distinct keys");
}
