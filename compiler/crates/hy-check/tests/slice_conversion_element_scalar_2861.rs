//! slice_conversion_element_scalar_2861.rs — issue #2861 (Row 13 BUILDABLE).
//!
//! `OpKind` collapses every integer width into `Int` and both float widths
//! into `Float`, so `conversion_kinds_conflict`'s `(Slice(a), Slice(b))` arm
//! compared `int32`- and `int64`-element slices as equal and accepted them:
//!
//! ```hy
//! type int_slice_a = []int32
//! type int_slice_b = []int64
//! var a int_slice_a
//! var b int_slice_b = int_slice_b(a)   // Go: cannot convert a to type int_slice_b
//! ```
//!
//! Go requires the two element types of a slice conversion to be IDENTICAL, so
//! the declared element scalar now decides once the kinds are equal — for both
//! the named conversion path and the `[]T(x)` slice-literal path. Genuine
//! aliases (`byte`/`uint8`, `rune`/`int32`) share one canonical name and stay
//! convertible, which the accept side pins.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects_with_e1002(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {:?}, got {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts_without_e1002(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E1002"),
        "must NOT fire HYB-E1002 for {:?}, got {:?}",
        src,
        cs
    );
}

// ------------------------------------------------------------ reject (#2861):

#[test]
fn aliased_slice_conversion_with_a_wider_element_rejects_e1002() {
    // The issue's reproducer, verbatim.
    rejects_with_e1002(
        "type int_slice_a = []int32\ntype int_slice_b = []int64\n\nfn main() {\n    var a int_slice_a\n    var b int_slice_b = int_slice_b(a)\n    fmt.println(\"{b}\")\n}",
    );
}

#[test]
fn direct_slice_conversion_with_mismatched_elements_rejects_e1002() {
    rejects_with_e1002(
        "fn main() {\n    var a []int32\n    var b []int64 = []int64(a)\n    fmt.println(\"{b}\")\n}",
    );
    rejects_with_e1002(
        "fn main() {\n    var a []float32\n    var b []float64 = []float64(a)\n    fmt.println(\"{b}\")\n}",
    );
    // Different signedness is a different type, not an alias.
    rejects_with_e1002(
        "fn main() {\n    var a []int32\n    var b []uint32 = []uint32(a)\n    fmt.println(\"{b}\")\n}",
    );
}

#[test]
fn slice_literal_conversion_with_mismatched_elements_rejects_e1002() {
    rejects_with_e1002(
        "fn main() {\n    b := []int64([]int32{1, 2})\n    fmt.println(\"{b}\")\n}",
    );
    rejects_with_e1002(
        "fn main() {\n    b := []float64([]float32{1.0})\n    fmt.println(\"{b}\")\n}",
    );
}

// ------------------------------------------------------------ accept:

#[test]
fn identical_element_types_stay_accepted() {
    accepts_without_e1002(
        "fn main() {\n    var a []int32\n    var b []int32 = []int32(a)\n    fmt.println(\"{b}\")\n}",
    );
    accepts_without_e1002(
        "type int_slice_a = []int32\nfn main() {\n    var a int_slice_a\n    b := int_slice_a(a)\n    fmt.println(\"{b}\")\n}",
    );
    accepts_without_e1002(
        "fn main() {\n    b := []int64([]int64{1, 2})\n    fmt.println(\"{b}\")\n}",
    );
}

#[test]
fn byte_and_rune_aliases_stay_convertible() {
    // `byte` IS `uint8` and `rune` IS `int32`, so both conversions build in Go.
    accepts_without_e1002(
        "fn main() {\n    var a []byte\n    var b []uint8 = []uint8(a)\n    fmt.println(\"{b}\")\n}",
    );
    accepts_without_e1002(
        "fn main() {\n    var a []rune\n    var b []int32 = []int32(a)\n    fmt.println(\"{b}\")\n}",
    );
    accepts_without_e1002(
        "fn main() {\n    a := []rune{'x'}\n    b := []int32(a)\n    fmt.println(\"{b}\")\n}",
    );
}

#[test]
fn string_conversions_stay_accepted() {
    // The `string` <-> `[]byte` pair is not a slice-to-slice conversion.
    accepts_without_e1002(
        "fn main() {\n    v := []byte(\"abc\")\n    fmt.println(\"{v}\")\n}",
    );
    accepts_without_e1002(
        "fn main() {\n    var bs []byte\n    s := string(bs)\n    fmt.println(\"{s}\")\n}",
    );
}
