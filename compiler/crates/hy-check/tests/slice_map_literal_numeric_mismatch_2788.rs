//! slice_map_literal_numeric_mismatch_2788.rs — issue #2788 (Row 13 BUILDABLE).
//!
//! `check_slice_lit_items` / `check_array_lit_items` / `check_map_lit_entries`
//! short-circuited on `ik == expected`, and every integer width collapses to
//! `Int` while both float widths collapse to `Float`. A TYPED element/key/
//! value of another concrete numeric type (`int` into `int64`, `float64` into
//! `float32`) therefore passed `hyb check` and failed `go build` (`cannot use
//! i (variable of type int) as int64 value in array or slice literal`).
//! Untyped constants and the byte/uint8 · rune/int32 aliases stay accepted.

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
fn mismatched_numeric_literal_items_reject() {
    // The issue's own minimal reproducer.
    let r = check(
        "fn main() {\n    var i int = 42\n    var f float64 = 3.14\n    _ = []int64{ i }\n    _ = []float32{ f }\n    _ = map[int64]string{ i: \"a\" }\n    _ = map[string]int64{ \"a\": i }\n    _ = map[float32]string{ f: \"a\" }\n    _ = map[string]float32{ \"a\": f }\n}\n",
    );
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().filter(|c| *c == "HYB-E1002").count() >= 6,
        "expected at least six HYB-E1002, got {codes:?}"
    );

    for (name, src) in [
        (
            "array literal element",
            "fn main() {\n    var i int = 42\n    _ = [2]int64{ i, i }\n}\n",
        ),
        (
            "slice element, float32 target",
            "fn main() {\n    var f float64 = 1.0\n    _ = []float32{ f }\n}\n",
        ),
        (
            "slice element, int32 target from int64 var",
            "fn main() {\n    var n int64 = 1\n    _ = []int32{ n }\n}\n",
        ),
        (
            "map value, int64 target from int var",
            "fn main() {\n    var i int = 1\n    _ = map[string]int64{ \"a\": i }\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "{name}");
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "{name}: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn untyped_constants_aliases_and_exact_widths_stay_accepted() {
    for (name, src) in [
        (
            "untyped constants adapt in slice and map",
            "fn main() {\n    _ = []int64{ 42 }\n    _ = []float32{ 1.5 }\n    _ = map[int64]string{ 1: \"a\" }\n    _ = map[string]int64{ \"a\": 1 }\n}\n",
        ),
        (
            "byte/uint8 alias",
            "fn main() {\n    var b byte = 1\n    _ = []uint8{ b }\n    _ = map[uint8]string{ b: \"a\" }\n}\n",
        ),
        (
            "rune/int32 alias",
            "fn main() {\n    var r rune = 2\n    _ = []int32{ r }\n    _ = map[string]int32{ \"a\": r }\n}\n",
        ),
        (
            "exact widths",
            "fn main() {\n    var i int64 = 1\n    var f float32 = 1.0\n    _ = []int64{ i }\n    _ = []float32{ f }\n    _ = map[string]float32{ \"a\": f }\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "{name}: {:?}", r.diagnostics);
        assert!(error_codes(&r).is_empty(), "{name}: {:?}", r.diagnostics);
    }
}
