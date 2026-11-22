//! range_counter_width.rs — the counter of `for i in lo..hi` has a Go type:
//! the emitter lowers it to `i := lo`, so it is `lo`'s declared width, or
//! `int` when `lo` is an untyped literal. A counter used where a different
//! concrete numeric type is required emits Go that fails `go build`
//! (`cannot use i (variable of type int) as int64 value ...`), so `hyb check`
//! must reject it with HYB-E1002 (GUARANTEES row 13, BUILDABLE).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn has_e1002(r: &hy_check::CheckResult) -> bool {
    r.diagnostics
        .iter()
        .any(|d| d.severity == hy_diag::Severity::Error && d.code == "HYB-E1002")
}

#[test]
fn untyped_range_counter_is_int_and_rejects_other_widths() {
    let cases = [
        // var initializer
        "fn main() {\n    for i in 0..3 {\n        var y int64 = i\n        fmt.println(y)\n    }\n}\n",
        // inclusive form
        "fn main() {\n    for i in 0..=3 {\n        var y int64 = i\n        fmt.println(y)\n    }\n}\n",
        // untyped literal lo, int-typed local hi
        "fn main() {\n    n := 5\n    for i in 0..n {\n        var y int64 = i\n        fmt.println(y)\n    }\n}\n",
        // call argument
        "fn take(a int64) int64 { return a }\nfn main() {\n    for i in 0..3 {\n        fmt.println(take(i))\n    }\n}\n",
        // assignment
        "fn main() {\n    var y int8 = 1\n    for i in 0..3 {\n        y = i\n    }\n    fmt.println(y)\n}\n",
        // mixed-width arithmetic
        "fn main() {\n    for i in 0..3 {\n        y := uint8(1) + i\n        fmt.println(y)\n    }\n}\n",
        // slice literal element
        "fn main() {\n    for i in 0..3 {\n        s := []int64{i}\n        fmt.println(s)\n    }\n}\n",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:\n{}", src);
        assert!(has_e1002(&r), "expected HYB-E1002 for:\n{}", src);
    }
}

#[test]
fn typed_range_counter_takes_lo_width() {
    let reject = [
        "fn main() {\n    var lo u8 = 1\n    for i in lo..5 {\n        var y int = i\n        fmt.println(y)\n    }\n}\n",
        "fn main() {\n    var lo int64 = 1\n    for i in lo..5 {\n        y := i + int32(1)\n        fmt.println(y)\n    }\n}\n",
    ];
    for src in reject {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:\n{}", src);
        assert!(has_e1002(&r), "expected HYB-E1002 for:\n{}", src);
    }
    let accept = "fn main() {\n    var lo u8 = 1\n    for i in lo..5 {\n        var y u8 = i\n        fmt.println(y + i)\n    }\n}\n";
    let r = check(accept);
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn range_counter_matching_widths_still_accepts() {
    let cases = [
        "fn main() {\n    for i in 0..3 {\n        var y int = i\n        z := i + 1\n        fmt.println(y, z)\n    }\n}\n",
        "fn main() {\n    for i in 0..3 {\n        fmt.println(float64(i), int64(i))\n    }\n}\n",
        "fn take(a int) int { return a }\nfn main() {\n    for i in 0..=3 {\n        fmt.println(take(i))\n    }\n}\n",
        // the counter shadows an outer wider binding: the outer width must not leak in
        "fn main() {\n    i := int64(1)\n    for i in 0..3 {\n        var y int = i\n        fmt.println(y)\n    }\n    var z int64 = i\n    fmt.println(z)\n}\n",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "expected acceptance for:\n{}\n{:?}", src, r.diagnostics);
    }
}
