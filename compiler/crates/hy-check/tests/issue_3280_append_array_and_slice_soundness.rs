//! issue_3280_append_array_and_slice_soundness.rs — regression tests for issue #3280 (Row 13 BUILDABLE).
//!
//! `check_append_elements` failed to verify fixed-size array dimensions and nested slice
//! element scalar compatibility between the destination slice and appended values (both
//! discrete elements and variadic spread), allowing type mismatches (such as `[3]int` into
//! `[][2]int` or `[]int32` into `[][]int64`) to pass `hyb check` with 0 diagnostics and fail
//! downstream `go build`.

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
fn mismatched_fixed_size_array_dimension_append_rejects() {
    for (name, src) in [
        (
            "[3]int variable into [][2]int",
            "fn main() {\n    var s [][2]int\n    var a [3]int\n    s = append(s, a)\n    _ = s\n}\n",
        ),
        (
            "[2]int32 variable into [][2]int64 (element scalar mismatch)",
            "fn main() {\n    var s [][2]int64\n    var a [2]int32\n    s = append(s, a)\n    _ = s\n}\n",
        ),
        (
            "[3]int literal into [][2]int",
            "fn main() {\n    var s [][2]int\n    s = append(s, [3]int{1, 2, 3})\n    _ = s\n}\n",
        ),
        (
            "[][3]int spread into [][2]int",
            "fn main() {\n    var s [][2]int\n    var b [][3]int\n    s = append(s, b...)\n    _ = s\n}\n",
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
fn mismatched_nested_slice_scalar_append_rejects() {
    for (name, src) in [
        (
            "[]int32 variable into [][]int64",
            "fn main() {\n    var s [][]int64\n    var item []int32\n    s = append(s, item)\n    _ = s\n}\n",
        ),
        (
            "[]int variable into [][]int64",
            "fn main() {\n    var s [][]int64\n    var item []int\n    s = append(s, item)\n    _ = s\n}\n",
        ),
        (
            "[]int32 literal into [][]int64",
            "fn main() {\n    var s [][]int64\n    s = append(s, []int32{1, 2})\n    _ = s\n}\n",
        ),
        (
            "[][]int32 spread into [][]int64",
            "fn main() {\n    var s [][]int64\n    var b [][]int32\n    s = append(s, b...)\n    _ = s\n}\n",
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
fn matching_array_and_nested_slice_append_accepts() {
    for (name, src) in [
        (
            "matching [2]int array into [][2]int",
            "fn main() {\n    var s [][2]int\n    var a [2]int\n    s = append(s, a)\n    _ = s\n}\n",
        ),
        (
            "matching hybrid aliases [2]i64 into [][2]int64",
            "fn main() {\n    var s [][2]int64\n    var a [2]i64\n    s = append(s, a)\n    _ = s\n}\n",
        ),
        (
            "matching nested slice []int64 into [][]int64",
            "fn main() {\n    var s [][]int64\n    var item []int64\n    s = append(s, item)\n    _ = s\n}\n",
        ),
        (
            "matching hybrid aliases []i64 into [][]int64",
            "fn main() {\n    var s [][]int64\n    var item []i64\n    s = append(s, item)\n    _ = s\n}\n",
        ),
        (
            "matching spread [][2]int into [][2]int",
            "fn main() {\n    var s [][2]int\n    var b [][2]int\n    s = append(s, b...)\n    _ = s\n}\n",
        ),
        (
            "matching spread [][]int64 into [][]int64",
            "fn main() {\n    var s [][]int64\n    var b [][]int64\n    s = append(s, b...)\n    _ = s\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "{name}: {:?}", r.diagnostics);
        assert!(error_codes(&r).is_empty(), "{name}: {:?}", r.diagnostics);
    }
}
#[test]
fn generic_user_method_call_iterable_stays_accepted() {
    let src = "\
struct cache[k comparable] {
    items []k
}

fn (c *cache[k]) keys() []k {
    return c.items
}

fn main() {
    c := cache[int]{items: []int{1, 2}}
    for v in c.keys() {
        fmt.println(v)
    }
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected clean, got: {:?}", r.diagnostics);
    assert!(error_codes(&r).is_empty(), "expected no errors, got: {:?}", r.diagnostics);
}
