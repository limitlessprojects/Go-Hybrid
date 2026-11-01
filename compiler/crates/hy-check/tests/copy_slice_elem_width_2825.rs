//! copy_slice_elem_width_2825.rs — tests for HYB-E1002 rejection of `copy`
//! arguments whose slice element types differ only in numeric WIDTH, and of
//! `string` sources copied into non-`[]byte` destinations (issue #2825).
//!
//! `copy_element_kinds_conflict` compares `OpKind`s, which collapse every
//! integer width into `Int` and both float widths into `Float`, and the
//! `string`-source arm accepted every integer element. Both escape the
//! BUILDABLE guarantee (GUARANTEES.md Row 13): `go build` refuses
//! `copy([]int64, []int32)` with `arguments to copy have different element
//! types`, and refuses copying a `string` into anything but `[]byte`/`[]uint8`.
//!
//! The width-aware comparison resolves named slice aliases (`my_int64_slice`
//! → `int64`), so the issue's exact typed-declaration reproducer is pinned,
//! and the byte/rune canonical spellings (`byte`=`uint8`, `rune`=`int32`)
//! must never produce false positives.

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
// Rejects: width-mismatched slice↔slice copies
// ---------------------------------------------------------------------------

#[test]
fn copy_int64_dst_int32_src_rejects_width_mismatch() {
    rejects_e1002(
        "\
fn main() {
    var dst []int64
    var src []int32
    _ = copy(dst, src)
}
",
    );
}

#[test]
fn copy_int32_dst_int64_src_rejects_width_mismatch() {
    rejects_e1002(
        "\
fn main() {
    var dst []int32
    var src []int64
    _ = copy(dst, src)
}
",
    );
}

#[test]
fn copy_float64_dst_float32_src_rejects_width_mismatch() {
    rejects_e1002(
        "\
fn main() {
    var dst []float64
    var src []float32
    _ = copy(dst, src)
}
",
    );
}

#[test]
fn copy_int_dst_int64_src_rejects_width_mismatch() {
    rejects_e1002(
        "\
fn main() {
    var dst []int
    var src []int64
    _ = copy(dst, src)
}
",
    );
}

#[test]
fn copy_make_mixed_widths_rejects() {
    rejects_e1002(
        "\
fn main() {
    var dst = make([]uint16, 4)
    var src = make([]uint32, 4)
    _ = copy(dst, src)
}
",
    );
}

// Named slice aliases: the issue's exact reproducer — `OpKind` is `Slice(Int)`
// on both sides, so only alias resolution through the registry catches it.

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
// Rejects: string sources into non-byte integer slices
// ---------------------------------------------------------------------------

#[test]
fn copy_string_into_int_slice_rejects() {
    rejects_e1002(
        "\
fn main() {
    var dst []int = []int{1, 2}
    var str string = \"hi\"
    _ = copy(dst, str)
}
",
    );
}

#[test]
fn copy_string_into_int64_slice_rejects() {
    rejects_e1002(
        "\
fn main() {
    var dst []int64 = []int64{1, 2}
    _ = copy(dst, \"hi\")
}
",
    );
}

#[test]
fn copy_string_into_rune_slice_rejects() {
    // `[]rune` is `[]int32`: a value copy of UTF-8 bytes into code points is
    // not what Go builds — only `[]byte`/`[]uint8` may take a string source.
    rejects_e1002(
        "\
fn main() {
    var dst []rune = []rune{1, 2}
    _ = copy(dst, \"hi\")
}
",
    );
}

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

// ---------------------------------------------------------------------------
// Accepts: identical element types, including canonical aliases
// ---------------------------------------------------------------------------

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
fn copy_string_into_byte_slice_accepted() {
    accepts(
        "\
fn main() {
    var dst []byte = []byte{0, 0}
    _ = copy(dst, \"hi\")
}
",
    );
}

#[test]
fn copy_string_into_uint8_slice_accepted() {
    // `byte` and `uint8` are one canonical type — no false positive.
    accepts(
        "\
fn main() {
    var dst []uint8 = []uint8{0, 0}
    _ = copy(dst, \"hi\")
}
",
    );
}

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
fn copy_byte_dst_uint8_src_accepted() {
    accepts(
        "\
fn main() {
    var dst []byte = []byte{0, 0}
    var src []uint8 = []uint8{1, 2}
    _ = copy(dst, src)
}
",
    );
}

#[test]
fn copy_rune_dst_int32_src_accepted() {
    accepts(
        "\
fn main() {
    var dst []rune = []rune{1, 2}
    var src []int32 = []int32{3, 4}
    _ = copy(dst, src)
}
",
    );
}

#[test]
fn copy_same_int_width_variable_and_literal_accepted() {
    accepts(
        "\
fn main() {
    var dst []int64 = []int64{1, 2}
    _ = copy(dst, []int64{3, 4})
}
",
    );
}
