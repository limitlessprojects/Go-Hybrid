//! append_numeric_mismatch_2796.rs — issue #2796 (Row 13 BUILDABLE).
//!
//! `check_append_elements` compared only `OpKind`, and every integer width
//! maps to `Int` while both float widths map to `Float`, so appending a TYPED
//! value of another concrete numeric type passed `hyb check` and failed
//! `go build` (`cannot use y (variable of type int) as int64 value in argument
//! to append`). The variadic-spread arm (#2738) already compared concrete
//! spellings; this closes the individual-element arm. Untyped constants and
//! the byte/uint8 · rune/int32 aliases stay accepted.

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
fn mismatched_concrete_numeric_append_elements_reject() {
    // The issue's own reproducer: `int` into `[]int64`.
    let r = check(
        "fn main() {\n    var xs []int64 = []int64{1, 2}\n    var y int = 3\n    xs = append(xs, y)\n    _ = xs\n}\n",
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );

    for (name, src) in [
        (
            "float32 into []float64",
            "fn main() {\n    var fs []float64 = []float64{1.0}\n    var f float32 = 2.0\n    fs = append(fs, f)\n    _ = fs\n}\n",
        ),
        (
            "int64 into []int32",
            "fn main() {\n    var xs []int32 = []int32{1}\n    var y int64 = 2\n    xs = append(xs, y)\n    _ = xs\n}\n",
        ),
        (
            "uint64 into []uint32",
            "fn main() {\n    var xs []uint32 = []uint32{1}\n    var y uint64 = 2\n    xs = append(xs, y)\n    _ = xs\n}\n",
        ),
        (
            "hybrid spelling i64 destination",
            "fn main() {\n    var xs []i64 = []i64{1}\n    var y int = 2\n    xs = append(xs, y)\n    _ = xs\n}\n",
        ),
        (
            "second element of a multi-element append",
            "fn main() {\n    var xs []int64 = []int64{1}\n    var y int64 = 2\n    var z int = 3\n    xs = append(xs, y, z)\n    _ = xs\n}\n",
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
            "untyped constants adapt",
            "fn main() {\n    var xs []int64 = []int64{1}\n    var fs []float64 = []float64{1.0}\n    xs = append(xs, 2)\n    fs = append(fs, 2.5)\n    _ = xs\n    _ = fs\n}\n",
        ),
        (
            "exact widths",
            "fn main() {\n    var xs []int64 = []int64{1}\n    var y int64 = 2\n    var fs []float64 = []float64{1.0}\n    var f float64 = 2.0\n    xs = append(xs, y)\n    fs = append(fs, f)\n    _ = xs\n    _ = fs\n}\n",
        ),
        (
            "byte/uint8 and rune/int32 aliases",
            "fn main() {\n    var bs []byte = []byte{1}\n    var b byte = 2\n    bs = append(bs, b)\n    var rs []int32 = []int32{1}\n    var r rune = 2\n    rs = append(rs, r)\n    _ = bs\n    _ = rs\n}\n",
        ),
        (
            "variadic spread still behaves",
            "fn main() {\n    var xs []int64 = []int64{1}\n    var ys []int64 = []int64{2}\n    xs = append(xs, ys...)\n    _ = xs\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "{name}: {:?}", r.diagnostics);
        assert!(error_codes(&r).is_empty(), "{name}: {:?}", r.diagnostics);
    }
}
