//! issue_2858_typed_index_field_scalars.rs — tests for HYB-E1002 rejection
//! of mismatched typed integer values read through index, map lookup, field
//! selection, or string index expressions (fixes #2858).

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
fn typed_integers_read_through_slice_or_array_index_rejects_e1002() {
    let cases = [
        "fn main() {\n    s := []i64{1}\n    var a int = s[0]\n    _ = a\n}\n",
        "fn main() {\n    s := []i64{1}\n    var a i32 = s[0]\n    _ = a\n}\n",
        "fn main() {\n    s := []u8{1}\n    var a i8 = s[0]\n    _ = a\n}\n",
        "fn main() {\n    var s []i64\n    var a int = s[0]\n    _ = a\n}\n",
        "fn main() {\n    arr := [2]i64{1, 2}\n    var a int = arr[0]\n    _ = a\n}\n",
        "fn main() {\n    var arr [2]i64\n    var a int = arr[0]\n    _ = a\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:\n{}", src);
        assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    }
}

#[test]
fn typed_integers_read_through_map_lookup_rejects_e1002() {
    let cases = [
        "fn main() {\n    m := map[string]i64{\"a\": 1}\n    var b int = m[\"a\"]\n    _ = b\n}\n",
        "fn main() {\n    m := map[string]i64{\"a\": 1}\n    var b i32 = m[\"a\"]\n    _ = b\n}\n",
        "fn main() {\n    var m map[string]i64\n    var b int = m[\"a\"]\n    _ = b\n}\n",
        "fn main() {\n    m := map[string]i64{\"a\": 1}\n    val, ok := m[\"a\"]\n    var b int = val\n    _ = b\n    _ = ok\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:\n{}", src);
        assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    }
}

#[test]
fn typed_integers_read_through_struct_field_rejects_e1002() {
    let cases = [
        "struct pt {\n    v i64\n}\nfn main() {\n    p := pt{v: 1}\n    var c int = p.v\n    _ = c\n}\n",
        "struct pt {\n    v i64\n}\nfn main() {\n    var p pt\n    var c i32 = p.v\n    _ = c\n}\n",
        "struct pt {\n    v i64\n}\nfn main() {\n    p := &pt{v: 1}\n    var c int = p.v\n    _ = c\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:\n{}", src);
        assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    }
}

#[test]
fn typed_integers_propagated_through_short_decl_rejects_e1002() {
    let cases = [
        "fn main() {\n    s := []i64{1}\n    x := s[0]\n    var d int = x\n    _ = d\n}\n",
        "fn main() {\n    m := map[string]i64{\"a\": 1}\n    x := m[\"a\"]\n    var d int = x\n    _ = d\n}\n",
        "struct pt {\n    v i64\n}\nfn main() {\n    p := pt{v: 1}\n    x := p.v\n    var d int = x\n    _ = d\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:\n{}", src);
        assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    }
}

#[test]
fn string_index_byte_rejects_non_byte_integers() {
    let cases = [
        "fn main() {\n    str := \"abc\"\n    var r rune = str[0]\n    _ = r\n}\n",
        "fn main() {\n    str := \"abc\"\n    var r int = str[0]\n    _ = r\n}\n",
        "fn main() {\n    str := \"abc\"\n    var r i64 = str[0]\n    _ = r\n}\n",
        "fn main() {\n    str := \"abc\"\n    b := str[0]\n    var r rune = b\n    _ = r\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:\n{}", src);
        assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    }
}

#[test]
fn index_and_field_in_binary_ops_and_comparisons_rejects_e1002() {
    let cases = [
        "fn main() {\n    s := []i64{1}\n    var y i8 = 5\n    c := s[0] + y\n    _ = c\n}\n",
        "fn main() {\n    s := []i64{1}\n    var y i8 = 5\n    c := s[0] == y\n    _ = c\n}\n",
        "struct pt {\n    v i64\n}\nfn main() {\n    p := pt{v: 1}\n    var y i32 = 2\n    c := p.v + y\n    _ = c\n}\n",
        "fn main() {\n    str := \"abc\"\n    var y i32 = 2\n    c := str[0] == y\n    _ = c\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:\n{}", src);
        assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    }
}

#[test]
fn matching_and_untyped_reads_accepted() {
    let cases = [
        "fn main() {\n    s := []i64{1}\n    var a i64 = s[0]\n    _ = a\n}\n",
        "fn main() {\n    m := map[string]i64{\"a\": 1}\n    var b i64 = m[\"a\"]\n    _ = b\n}\n",
        "struct pt {\n    v i64\n}\nfn main() {\n    p := pt{v: 1}\n    var c i64 = p.v\n    _ = c\n}\n",
        "fn main() {\n    str := \"abc\"\n    var b byte = str[0]\n    _ = b\n}\n",
        "fn main() {\n    str := \"abc\"\n    var u uint8 = str[0]\n    _ = u\n}\n",
        "fn main() {\n    untyped := [1, 2]\n    var a int = untyped[0]\n    _ = a\n}\n",
        "fn main() {\n    s := []i64{1}\n    x := s[0]\n    var d i64 = x\n    _ = d\n}\n",
        "fn main() {\n    s := []i64{1}\n    c := s[0] + 1\n    _ = c\n}\n",
        "fn main() {\n    s := []i64{1}\n    for x in s {\n        var a i64 = x\n        _ = a\n    }\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "expected accept for:\n{}", src);
        assert_eq!(error_codes(&r).len(), 0);
    }
}

#[test]
fn issue_2858_exact_reproducer_rejects_e1002() {
    let src = "struct pt {\n    v i64\n}\nfn main() {\n    s := []i64{1}\n    m := map[string]i64{\"a\": 1}\n    p := pt{v: 1}\n    var a int = s[0]\n    var b int = m[\"a\"]\n    var c int = p.v\n    x := s[0]\n    var d int = x\n    str := \"abc\"\n    var r rune = str[0]\n    fmt.println(a, b, c, d, r)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}
