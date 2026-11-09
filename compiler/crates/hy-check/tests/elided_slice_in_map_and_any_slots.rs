//! Soundness (GUARANTEES Row 13 — BUILDABLE & Row 6 — ARITH/EMIT-GUARD):
//! Distinct verification and emit-guard soundness battery for elided `[...]`
//! literals inside map entries, type-aliased composite slots, parenthesized
//! slot expressions, and multi-level slice/array slots (#3437 / #3438):
//!
//! 1. Type-aliased and parenthesized elided slice literals in map/slice/array
//!    and `any` slots (`HYB-E1002`).
//! 2. Fixed-size array bound overflow (`HYB-E6004`) on elided slice literals
//!    in map keys/values (`map[string][2]int{"a": [1, 2, 3]}`,
//!    `map[[2]int]string{[1, 2, 3]: "ok"}`, and type-aliased `[2]int` slots),
//!    which `hy-emit` lowers to `[2]int{1, 2, 3}` (`index 2 is out of bounds`).
//! 3. Constant integer overflow (`HYB-E6002`) inside elided slice literals in
//!    map slice/array entries (`map[string][]uint8{"a": [300]}`,
//!    `map[string][2]uint8{"a": [1, 300]}`) and nested slice slots
//!    (`[][]uint8{[300]}`, `[][][]uint8{[[300]]}`).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).unwrap_or_else(|d| panic!("must parse: {d:?}"));
    check_file(&ast)
}

fn errors(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect()
}

#[test]
fn type_aliased_and_parenthesized_elided_slice_slots_enforce_e1002() {
    for src in [
        r#"
type int_slice = []int
fn main() {
    m := map[string]int_slice{"a": ([1, "bad"])}
    _ = m
}
"#,
        r#"
type int_pair = [2]int
fn main() {
    var x i64 = 9
    m := map[string]int_pair{"a": ([1, x])}
    _ = m
}
"#,
        r#"
type any_val = any
fn main() {
    var x int = 5
    m := map[string]any_val{"a": ([x])}
    _ = m
}
"#,
        r#"
type any_val = any
fn main() {
    xs := []any_val{([1, "bad"])}
    _ = xs
}
"#,
    ] {
        let r = check(src);
        let errs = errors(&r);
        assert_eq!(r.verdict, Verdict::Reject, "must reject:
{src}");
        assert_eq!(
            errs.len(),
            1,
            "expected exactly 1 HYB-E1002 diagnostic for:
{src}
got {errs:?}"
        );
        assert!(errs[0].starts_with("HYB-E1002"), "got {errs:?}");
    }
}

#[test]
fn map_and_nested_array_slots_reject_elided_slice_exceeding_array_bounds() {
    for src in [
        r#"
fn main() {
    m := map[string][2]int{"a": [1, 2, 3]}
    _ = m
}
"#,
        r#"
fn main() {
    m := map[[2]int]string{[1, 2, 3]: "ok"}
    _ = m
}
"#,
        r#"
type int_pair = [2]int
fn main() {
    m := map[string]int_pair{"a": ([1, 2, 3])}
    _ = m
}
"#,
        r#"
fn main() {
    xs := [][2]int{[1, 2, 3]}
    _ = xs
}
"#,
        r#"
fn main() {
    arr := [1][2]int{[1, 2, 3]}
    _ = arr
}
"#,
    ] {
        let r = check(src);
        let errs = errors(&r);
        assert_eq!(r.verdict, Verdict::Reject, "must reject:
{src}");
        assert_eq!(
            errs.len(),
            1,
            "expected exactly 1 HYB-E6004 diagnostic for:
{src}
got {errs:?}"
        );
        assert!(errs[0].starts_with("HYB-E6004"), "got {errs:?}");
    }
}

#[test]
fn elided_slice_in_map_and_nested_slice_slots_reject_const_overflow() {
    for src in [
        r#"
fn main() {
    m := map[string][]uint8{"a": [300]}
    _ = m
}
"#,
        r#"
fn main() {
    m := map[string][2]uint8{"a": [1, 300]}
    _ = m
}
"#,
        r#"
fn main() {
    m := map[[2]uint8]string{[1, 300]: "ok"}
    _ = m
}
"#,
        r#"
fn main() {
    xs := [][]uint8{[300]}
    _ = xs
}
"#,
        r#"
fn main() {
    xs := [][][]uint8{[[300]]}
    _ = xs
}
"#,
    ] {
        let r = check(src);
        let errs = errors(&r);
        assert_eq!(r.verdict, Verdict::Reject, "must reject:
{src}");
        assert_eq!(
            errs.len(),
            1,
            "expected exactly 1 HYB-E6002 diagnostic for:
{src}
got {errs:?}"
        );
        assert!(errs[0].starts_with("HYB-E6002"), "got {errs:?}");
    }

    for good in [
        r#"
type u8_pair = [2]uint8
fn main() {
    m := map[string]u8_pair{"a": ([1, 255])}
    xs := [][][]uint8{[[255]]}
    _ = m
    _ = xs
}
"#,
        r#"
fn main() {
    m1 := map[string][2]int{"a": [1, 2]}
    m2 := map[[2]int]string{[1, 2]: "ok"}
    m3 := map[string]any{"a": [1, 2, 3]}
    _ = m1
    _ = m2
    _ = m3
}
"#,
    ] {
        let r = check(good);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "must accept:
{good}
got {:?}",
            errors(&r)
        );
    }
}
