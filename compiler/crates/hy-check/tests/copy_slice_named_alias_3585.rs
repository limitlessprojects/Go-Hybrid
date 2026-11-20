//! copy_slice_named_alias_3585.rs — tests for the named-slice-alias residual
//! family of issue #2825 (filed as #3585 after the DUP CHECK on PR #2940).
//!
//! Landed #3145 (fixes #3085) compares exact element spellings resolved from
//! the binding text, which covers anonymous `[]T` spellings. When a side is a
//! NAMED slice alias (`type bs = []byte`), its element spelling is not in the
//! binding text and must be resolved through the alias registry
//! (`named_slice_element_name`, the #2811 precedent) before the comparison —
//! otherwise every integer width collapses to `Int` / every float width to
//! `Float` (false negatives) and the string-source byte check falls back to
//! `int` and rejects the legal `[]byte`-alias destination (false positive).
//!
//! RED on `main` (d6507a52e) for the three shapes from #3585; GREEN with the
//! alias-resolution path. Canonical spellings (byte=uint8, rune=int32) stay
//! silent; unresolved elements and user types still fail open per the
//! #2811/#2788/#2796 convention.

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

fn rejects_e1002(src: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject, got {:?}", r);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

fn accepts(src: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept, got {:?}", r);
    assert!(
        error_codes(&r).is_empty(),
        "expected 0 errors, got {:?}",
        r.diagnostics
    );
}

// ---------------------------------------------------------------------------
// #3585 FN 1/2: width-mismatched copies where a side is a named slice alias
// ---------------------------------------------------------------------------

#[test]
fn copy_named_int64_slice_from_named_int32_slice_rejects() {
    rejects_e1002(
        "\
type my_int64_slice = []int64
type my_int32_slice = []int32

fn main() {
    var a_dst my_int64_slice = []int64{1, 2}
    var a_src my_int32_slice = []int32{3, 4}
    _ = copy(a_dst, a_src)
}
",
    );
}

#[test]
fn copy_named_slice_from_literal_other_width_rejects() {
    rejects_e1002(
        "\
type my_float64_slice = []float64

fn main() {
    var a_dst my_float64_slice = []float64{1.0, 2.0}
    _ = copy(a_dst, []float32{3.0, 4.0})
}
",
    );
}

// ---------------------------------------------------------------------------
// #3585 FP: the legal string source into a named []byte alias stays silent
// ---------------------------------------------------------------------------

#[test]
fn copy_named_byte_slice_from_string_accepted() {
    accepts(
        "\
type bs = []byte

fn main() {
    var dst bs = []byte{0, 0}
    _ = copy(dst, \"hi\")
}
",
    );
}

#[test]
fn copy_same_named_slice_type_accepted() {
    accepts(
        "\
type my_int64_slice = []int64

fn main() {
    var a_dst my_int64_slice = []int64{1, 2}
    var a_src my_int64_slice = []int64{3, 4}
    _ = copy(a_dst, a_src)
}
",
    );
}

#[test]
fn copy_named_rune_slice_from_int32_slice_accepted() {
    // `rune` and `int32` share one canonical name — alias resolution must not
    // manufacture a width conflict out of spelling differences alone.
    accepts(
        "\
type my_rune_slice = []rune

fn main() {
    var dst my_rune_slice = []rune{1, 2}
    var src []int32 = []int32{3, 4}
    _ = copy(dst, src)
}
",
    );
}

// ---------------------------------------------------------------------------
// Pins the pre-existing named rejection (element resolved but non-byte):
// the alias-resolution path must not silently widen string destinations.
// ---------------------------------------------------------------------------

#[test]
fn copy_string_into_named_int_slice_rejects() {
    rejects_e1002(
        "\
type my_int_slice = []int

fn main() {
    var dst my_int_slice = []int{1, 2}
    _ = copy(dst, \"hi\")
}
",
    );
}
