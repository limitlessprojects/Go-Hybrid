//! issue_3161_map_key_scalar_widths.rs — issue #3161 (Row 13 BUILDABLE).
//!
//! `check_index_kind` on `OpKind::Map` and `check_builtin_operand_kinds` on
//! `delete(m, k)` / `delete(s, v)` previously compared coarse `OpKind`
//! (`Int == Int`), letting mismatched typed keys (e.g. `k: int64` into
//! `map[int32]string` or `set[int32]`) pass `hyb check` and fail `go build`
//! (`cannot use k (variable of type int64) as int32 value in map index`).
//! Untyped numeric constants and matching scalar widths remain accepted.

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
fn map_key_scalar_mismatches_reject() {
    for (name, src, expected_code) in [
        (
            "map index read with int64 on int32 map",
            "fn main() {\n    var m map[int32]string\n    var k int64 = 1\n    _ = m[k]\n}\n",
            "HYB-E1002",
        ),
        (
            "map index read with int32 on int64 map",
            "fn main() {\n    var m map[int64]string\n    var k int32 = 1\n    _ = m[k]\n}\n",
            "HYB-E1002",
        ),
        (
            "map index read with float64 on float32 map",
            "fn main() {\n    var m map[float32]string\n    var f float64 = 1.0\n    _ = m[f]\n}\n",
            "HYB-E1002",
        ),
        (
            "map index write with int64 on int32 map",
            "fn main() {\n    var m map[int32]string\n    var k int64 = 1\n    m[k] = \"hello\"\n}\n",
            "HYB-E1002",
        ),
        (
            "map index write with int on int64 map",
            "fn main() {\n    var m map[int64]string\n    var k int = 1\n    m[k] = \"hello\"\n}\n",
            "HYB-E1002",
        ),
        (
            "delete builtin with int64 on int32 map",
            "fn main() {\n    var m map[int32]string\n    var k int64 = 1\n    delete(m, k)\n}\n",
            "HYB-E2005",
        ),
        (
            "delete builtin with int32 on int64 map",
            "fn main() {\n    var m map[int64]string\n    var k int32 = 1\n    delete(m, k)\n}\n",
            "HYB-E2005",
        ),
        (
            "delete builtin on set[int32] with int64 elem",
            "fn main() {\n    var s set[int32]\n    var v int64 = 1\n    delete(s, v)\n}\n",
            "HYB-E2005",
        ),
        (
            "struct field map index with mismatched key",
            "struct store {\n    m map[int32]string\n}\nfn main() {\n    var s store\n    var k int64 = 1\n    _ = s.m[k]\n}\n",
            "HYB-E1002",
        ),
        (
            "short decl map literal index with mismatched key",
            "fn main() {\n    m := map[int32]string{}\n    var k int64 = 1\n    _ = m[k]\n}\n",
            "HYB-E1002",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for {name}");
        let codes = error_codes(&r);
        assert!(
            codes.contains(&expected_code.to_string()),
            "expected {expected_code} for {name}, got {codes:?}"
        );
    }
}

#[test]
fn map_key_matching_and_untyped_accepts() {
    for (name, src) in [
        (
            "untyped integer literal in map index",
            "fn main() {\n    var m map[int32]string\n    _ = m[1]\n}\n",
        ),
        (
            "matching int32 variable in map index",
            "fn main() {\n    var m map[int32]string\n    var k int32 = 1\n    _ = m[k]\n}\n",
        ),
        (
            "rune variable in int32 map index (canonical alias)",
            "fn main() {\n    var m map[int32]string\n    var r rune = 'a'\n    _ = m[r]\n}\n",
        ),
        (
            "untyped integer literal in delete() on map",
            "fn main() {\n    var m map[int32]string\n    delete(m, 1)\n}\n",
        ),
        (
            "untyped integer literal in delete() on set",
            "fn main() {\n    var s set[int32]\n    delete(s, 1)\n}\n",
        ),
        (
            "string map with string key",
            "fn main() {\n    var m map[string]int\n    _ = m[\"key\"]\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "expected accept for {name}, got diagnostics {:?}",
            r.diagnostics
        );
    }
}
