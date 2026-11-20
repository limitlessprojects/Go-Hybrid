//! Regression tests for #2870:
//! hy-check: map and channel type conversions allow incompatible key, value, and element types (HYB-E1002).
//! Under Go specification (§Conversions), conversions between map types or between channel types
//! require identical key, value, or element types.

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
fn map_conversions_with_mismatched_key_or_value_scalars_rejected_with_e1002() {
    let src = r#"
type map_a = map[string]int32
type map_b = map[string]int64
type map_s = map[string]string
type map_k1 = map[int32]string
type map_k2 = map[int64]string

fn main() {
    var ma map_a
    var mb map_b = map_b(ma)

    var ms map_s
    var mb2 map_b = map_b(ms)

    var mk1 map_k1
    var mk2 map_k2 = map_k2(mk1)

    _ = mb
    _ = mb2
    _ = mk2
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected reject for mismatched map conversions");
    let codes = error_codes(&r);
    assert!(
        codes.iter().filter(|c| *c == "HYB-E1002").count() >= 3,
        "expected at least 3 HYB-E1002 errors, got {codes:?}"
    );
}

#[test]
fn chan_conversions_with_mismatched_element_scalars_rejected_with_e1002() {
    let src = r#"
type chan_a = chan int32
type chan_b = chan int64
type chan_s = chan string

fn main() {
    var ca chan_a
    var cb chan_b = chan_b(ca)

    var cs chan_s
    var cb2 chan_b = chan_b(cs)

    _ = cb
    _ = cb2
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected reject for mismatched chan conversions");
    let codes = error_codes(&r);
    assert!(
        codes.iter().filter(|c| *c == "HYB-E1002").count() >= 2,
        "expected at least 2 HYB-E1002 errors, got {codes:?}"
    );
}

#[test]
fn multi_level_type_alias_conversions_rejected_when_underlying_types_differ() {
    let src = r#"
type my_int = int32
type other_int = int64
type map1 = map[string]my_int
type map2 = map[string]other_int
type map1_alias = map1
type map2_alias = map2

fn main() {
    var m1 map1_alias
    var m2 map2_alias = map2_alias(m1)
    _ = m2
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {codes:?}"
    );
}

#[test]
fn valid_map_and_chan_conversions_accepted() {
    let src = r#"
type map_x = map[string]int32
type map_y = map[string]int32

type chan_x = chan int32
type chan_y = chan int32

fn main() {
    var mx map_x
    var my map_y = map_y(mx)

    var cx chan_x
    var cy chan_y = chan_y(cx)

    _ = my
    _ = cy
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected accept for matching map/chan conversions");
    assert!(r.diagnostics.is_empty(), "expected 0 diagnostics, got {:?}", r.diagnostics);
}
