//! slice_string_conversion_2811.rs — issue #2811 (Row 13 BUILDABLE).
//!
//! Go §Conversions to and from a string type admits only `[]byte`/`[]uint8`
//! and `[]rune`/`[]int32`. `OpKind::Int` collapses every integer width, so
//! `string([]int64)` and a slice alias `my_int64_slice(s)` passed `hyb check`
//! and failed `go build` (`cannot convert ... to type string`). The
//! non-`Int` element kinds (`[]float64`, `[]string`, `[]bool`) were already
//! rejected; this closes the integer-width gap.

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
fn non_byte_rune_string_slice_conversions_reject() {
    // The issue's own reproducer.
    let r = check(
        "type my_int64_slice = []int64\ntype my_float_slice = []float64\nfn main() {\n    var is []int64 = []int64{1, 2}\n    var fs []float64 = []float64{1.0, 2.0}\n    var ss []string = []string{\"a\", \"b\"}\n    _ = string(is)\n    _ = string(fs)\n    _ = string(ss)\n    var s string = \"hello\"\n    _ = my_int64_slice(s)\n    _ = my_float_slice(s)\n}\n",
    );
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().filter(|c| *c == "HYB-E1002").count() >= 5,
        "expected at least five HYB-E1002, got {codes:?}"
    );

    for (name, src) in [
        (
            "string([]int)",
            "fn main() {\n    var is []int = []int{1, 2}\n    _ = string(is)\n}\n",
        ),
        (
            "string([]uint64)",
            "fn main() {\n    var us []uint64 = []uint64{1}\n    _ = string(us)\n}\n",
        ),
        (
            "string([]int32) is legal but []uint32 is not",
            "fn main() {\n    var us []uint32 = []uint32{1}\n    _ = string(us)\n}\n",
        ),
        (
            "alias of []int",
            "type ints = []int\nfn main() {\n    _ = ints(\"hello\")\n}\n",
        ),
        (
            "alias of []uint8-ish width",
            "type u16s = []uint16\nfn main() {\n    _ = u16s(\"hello\")\n}\n",
        ),
        (
            "string([2]int) array",
            "fn main() {\n    var arr [2]int = [2]int{65, 66}\n    _ = string(arr)\n}\n",
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
fn byte_rune_and_other_valid_conversions_stay_accepted() {
    for (name, src) in [
        (
            "string([]byte) and string([]rune)",
            "fn main() {\n    var bs []byte = []byte{65, 66}\n    var rs []rune = []rune{65}\n    _ = string(bs)\n    _ = string(rs)\n}\n",
        ),
        (
            "string([]uint8) and string([]int32)",
            "fn main() {\n    var us []uint8 = []uint8{65}\n    var i32s []int32 = []int32{65}\n    _ = string(us)\n    _ = string(i32s)\n}\n",
        ),
        (
            "[]byte(s) and []rune(s)",
            "fn main() {\n    _ = []byte(\"hello\")\n    _ = []rune(\"hello\")\n}\n",
        ),
        (
            "aliases of []byte / []rune",
            "type bytes = []byte\ntype runes = []rune\nfn main() {\n    _ = bytes(\"hello\")\n    _ = runes(\"hello\")\n}\n",
        ),
        (
            "string(int) code point and string(string)",
            "fn main() {\n    _ = string(65)\n    _ = string(\"hi\")\n}\n",
        ),
        (
            "same-element slice conversion",
            "type ints = []int\nfn main() {\n    var xs []int = []int{1, 2}\n    _ = ints(xs)\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "{name}: {:?}", r.diagnostics);
        assert!(error_codes(&r).is_empty(), "{name}: {:?}", r.diagnostics);
    }
}
