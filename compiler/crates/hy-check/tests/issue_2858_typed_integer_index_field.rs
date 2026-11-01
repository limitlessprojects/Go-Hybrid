//! Regression test suite for issue #2858 (Row 13 BUILDABLE soundness).
//!
//! Typed integers read through index (slice, array, map, string) or struct field
//! expressions must not be mixed across widths/signedness (e.g. `var x int = s[0]`
//! where `s` is `[]i64`, or `var r rune = str[0]`).

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
fn typed_integer_index_and_field_mismatches_reject() {
    for (name, src) in [
        (
            "slice index into mismatched int type",
            "fn main() {\n    s := []i64{1}\n    var a int = s[0]\n    _ = a\n}\n",
        ),
        (
            "map lookup into mismatched int type",
            "fn main() {\n    m := map[string]i64{\"a\": 1}\n    var b int = m[\"a\"]\n    _ = b\n}\n",
        ),
        (
            "struct field into mismatched int type",
            "struct pt {\n    v i64\n}\nfn main() {\n    p := pt{v: 1}\n    var c int = p.v\n    _ = c\n}\n",
        ),
        (
            "local binding from slice index propagates mismatch",
            "fn main() {\n    s := []i64{1}\n    x := s[0]\n    var d int = x\n    _ = d\n}\n",
        ),
        (
            "string index yields byte, rejects rune/int32",
            "fn main() {\n    str := \"abc\"\n    var r rune = str[0]\n    _ = r\n}\n",
        ),
        (
            "array index into mismatched int type",
            "fn main() {\n    arr := [3]i64{1, 2, 3}\n    var a int = arr[0]\n    _ = a\n}\n",
        ),
        (
            "slice element assigned to mismatched int target",
            "fn main() {\n    var a int = 0\n    s := []i64{1}\n    a = s[0]\n    _ = a\n}\n",
        ),
        (
            "map element assigned to mismatched int target",
            "fn main() {\n    var b int = 0\n    m := map[string]i64{\"a\": 1}\n    b = m[\"a\"]\n    _ = b\n}\n",
        ),
        (
            "struct field assigned to mismatched int target",
            "struct pt {\n    v i64\n}\nfn main() {\n    var c int = 0\n    p := pt{v: 1}\n    c = p.v\n    _ = c\n}\n",
        ),
        (
            "binary op with slice element and mismatched int",
            "fn main() {\n    var a int = 1\n    s := []i64{1}\n    _ = a + s[0]\n}\n",
        ),
        (
            "comparison between map element and mismatched int",
            "fn main() {\n    var b int = 1\n    m := map[string]i64{\"a\": 1}\n    _ = b == m[\"a\"]\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "{name}");
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "{name}: expected HYB-E1002, got {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn matching_integer_index_and_field_stay_accepted() {
    for (name, src) in [
        (
            "slice element matching int type",
            "fn main() {\n    s := []int{1}\n    var a int = s[0]\n    _ = a\n}\n",
        ),
        (
            "map element matching int type",
            "fn main() {\n    m := map[string]int{\"a\": 1}\n    var b int = m[\"a\"]\n    _ = b\n}\n",
        ),
        (
            "struct field matching int type",
            "struct pt {\n    v int\n}\nfn main() {\n    p := pt{v: 1}\n    var c int = p.v\n    _ = c\n}\n",
        ),
        (
            "string index assigned to byte (uint8)",
            "fn main() {\n    str := \"abc\"\n    var b byte = str[0]\n    _ = b\n}\n",
        ),
        (
            "string index assigned to uint8",
            "fn main() {\n    str := \"abc\"\n    var u uint8 = str[0]\n    _ = u\n}\n",
        ),
        (
            "untyped integer constants into slice and map",
            "fn main() {\n    s := []i64{1, 2, 3}\n    m := map[string]i64{\"k\": 42}\n    _ = s\n    _ = m\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "{name}: {:?}", r.diagnostics);
        assert!(error_codes(&r).is_empty(), "{name}: {:?}", r.diagnostics);
    }
}
