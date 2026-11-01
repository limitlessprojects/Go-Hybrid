//! Regression test suite for Issue #2870 (Row 13 BUILDABLE Soundness track).
//!
//! Under Go language specification (§Conversions):
//! "A non-constant value x can be converted to type T in any of these cases:
//!  - x's type and T have identical underlying types.
//!  - x is a bidirectional channel value, T is a channel type, x's type and T have identical element types,
//!    and at least one of x's type or T is not a named type."
//!
//! Conversions between map types with mismatched key or value scalars/types, or channel types
//! with mismatched element scalars/types, are illegal in Go and rejected with HYB-E1002.

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

fn assert_e1002(src: &str) {
    let cs = codes(src);
    assert!(
        !cs.is_empty() && cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 among diagnostics, got {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(
        cs.is_empty(),
        "expected 0 diagnostics, got {cs:?} for:\n{src}"
    );
}

#[test]
fn map_conversion_mismatched_value_scalars_rejected() {
    let src = r#"
type map_a = map[string]int32
type map_b = map[string]int64

fn main() {
    var ma map_a
    var mb map_b = map_b(ma)
    _ = mb
}
"#;
    assert_e1002(src);
}

#[test]
fn map_conversion_mismatched_key_scalars_rejected() {
    let src = r#"
type map_k1 = map[int32]string
type map_k2 = map[int64]string

fn main() {
    var m1 map_k1
    var m2 map_k2 = map_k2(m1)
    _ = m2
}
"#;
    assert_e1002(src);
}

#[test]
fn map_conversion_mismatched_kinds_rejected() {
    let src = r#"
type map_s = map[string]string
type map_b = map[string]int64

fn main() {
    var ms map_s
    var mb map_b = map_b(ms)
    _ = mb
}
"#;
    assert_e1002(src);
}

#[test]
fn channel_conversion_mismatched_element_scalars_rejected() {
    let src = r#"
type chan_a = chan int32
type chan_b = chan int64

fn main() {
    var ca chan_a
    var cb chan_b = chan_b(ca)
    _ = cb
}
"#;
    assert_e1002(src);
}

#[test]
fn channel_conversion_mismatched_kinds_rejected() {
    let src = r#"
type chan_s = chan string
type chan_b = chan int64

fn main() {
    var cs chan_s
    var cb chan_b = chan_b(cs)
    _ = cb
}
"#;
    assert_e1002(src);
}

#[test]
fn map_and_channel_conversions_with_identical_underlying_types_accepted() {
    let src = r#"
type map_base = map[string]int64
type my_map = map_base

type chan_base = chan int64
type my_chan = chan_base

fn main() {
    var mb map_base
    var m2 my_map = my_map(mb)

    var cb chan_base
    var c2 my_chan = my_chan(cb)

    _ = m2
    _ = c2
}
"#;
    assert_ok(src);
}
