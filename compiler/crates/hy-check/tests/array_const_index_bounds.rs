//! array_const_index_bounds.rs — regression tests for the constant-index
//! bound on a FIXED-SIZE ARRAY (GUARANTEES.md Row 13, BUILDABLE).
//!
//! `check_index_kind` bounded the constant index of the STRING form (`s[10]`
//! on `const s = "abc"`) but not the fixed-array form: `a[5]` on `[3]int`
//! checked clean and the emitted Go failed `go build` with
//! `invalid argument: index 5 out of bounds [0:3]` — an accept that cannot
//! build is exactly the breach Row 13 forbids.
//!
//! The bound is read from the array's TYPE, not from a foldable initializer:
//! `var a [3]int` is not a `const` binding and its length is certain anyway.
//! The slice form must stay accepted — a slice's length is not part of its
//! type, so no constant bound is decidable there, and bounding it would be an
//! over-reach (the corpus fence is
//! `control-slice-index-past-length.hy`).

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_reject_e1002(src: &str) -> CheckResult {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected REJECT for src:\n{src}\ndiagnostics: {:?}",
        r.diagnostics
    );
    assert!(
        codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for src:\n{src}\ngot {:?}",
        codes(&r)
    );
    r
}

fn assert_accept(src: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected ACCEPT for src:\n{src}\ndiagnostics: {:?}",
        r.diagnostics
    );
}

// ---------------------------------------------------------------------------
// Rejects — a constant index past a fixed array's declared length
// ---------------------------------------------------------------------------

#[test]
fn const_index_past_var_array_length_is_e1002() {
    let r = assert_reject_e1002(
        "fn main() {\n    var a [3]int\n    a[1] = 7\n    fmt.println(\"{a[5]}\")\n}\n",
    );
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.message.contains("out of bounds for a 3-element array")),
        "message must state the array's length: {:?}",
        r.diagnostics
    );
}

#[test]
fn const_index_equal_to_length_is_e1002() {
    // The boundary: `len(a)` is the first invalid index, so `a[3]` on `[3]int`
    // must be rejected while `a[2]` stays legal.
    assert_reject_e1002("fn main() {\n    var a [3]int\n    fmt.println(\"{a[3]}\")\n}\n");
}

#[test]
fn const_index_past_typed_array_literal_length_is_e1002() {
    assert_reject_e1002(
        "fn main() {\n    var a = [3]int{1, 2, 3}\n    fmt.println(\"{a[9]}\")\n}\n",
    );
}

#[test]
fn const_index_past_array_parameter_length_is_e1002() {
    assert_reject_e1002(
        "fn take(a [3]int) int {\n    return a[5]\n}\n\nfn main() {\n    var a [3]int\n    fmt.println(\"{take(a)}\")\n}\n",
    );
}

#[test]
fn const_index_past_const_string_length_is_still_e1002() {
    // The sibling rule, unchanged: the array branch must not have displaced it.
    assert_reject_e1002("fn main() {\n    const s = \"abc\"\n    fmt.println(\"{s[10]}\")\n}\n");
}

// ---------------------------------------------------------------------------
// Controls — in-bounds array indices and the slice form stay ACCEPT
// ---------------------------------------------------------------------------

#[test]
fn const_index_in_bounds_on_array_accepts() {
    assert_accept("fn main() {\n    var a [3]int\n    a[0] = 1\n    a[2] = 7\n    fmt.println(\"{a[0]}, {a[2]}\")\n}\n");
}

#[test]
fn const_index_past_slice_length_accepts() {
    // A slice's length is not part of its type: `xs[300]` is legal Go, so the
    // checker must not reject it. Guarded with `len(xs)` so the program also
    // exits 0 under the differential loop's run half.
    assert_accept(
        "fn main() {\n    xs := []int{1, 2}\n    if len(xs) > 300 {\n        fmt.println(\"{xs[300]}\")\n    }\n    fmt.println(\"{len(xs)}\")\n}\n",
    );
}

#[test]
fn large_array_length_control_accepts() {
    // corpus `control-large-array-length.hy`: a long array with no index at all
    // must stay clean (the bound only fires on a constant index).
    assert_accept("fn main() {\n    var a [300]int8\n    fmt.println(\"{len(a)}\")\n}\n");
}

// ---------------------------------------------------------------------------
// Issue #999: the assignment-target place (`a[300] = 1`) bypassed the bound —
// `place_read` passed no base expression to `check_index_kind`, so the
// declared array length was never resolved on the write path. The read path
// (`a[5]` above) was already bounded. The base place is now mirrored into
// expression form, so the write path resolves the length exactly like reads.
// ---------------------------------------------------------------------------

#[test]
fn const_index_assign_past_var_array_length_is_e1002() {
    assert_reject_e1002("fn main() {\n    var a [2]int\n    a[300] = 1\n    fmt.println(\"{a[0]}\")\n}\n");
}

#[test]
fn const_index_assign_in_bounds_still_accepts() {
    assert_accept("fn main() {\n    var a [2]int\n    a[1] = 1\n    fmt.println(\"{a[0]}\")\n}\n");
}

#[test]
fn const_index_assign_on_slice_still_accepts() {
    // Over-reach fence: a slice's length is not part of its type, so the
    // constant store past a slice's current length is a RUNTIME panic in Go,
    // not a compile error — this must stay accepted (mirrors
    // `control-slice-index-past-length.hy` in the corpus).
    assert_accept("fn main() {\n    var s []int = [1, 2]\n    s[300] = 1\n    _ = s\n}\n");
}
