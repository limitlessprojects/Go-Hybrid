//! issue_3166_multireturn_assign_scalar_widths.rs — issue #3166 (Row 13 BUILDABLE).
//!
//! Multi-value assignments (`a, b = f()`) and multi-value declarations
//! (`var a, b int32 = f()`) previously compared coarse `OpKind`s, letting
//! mismatched numeric widths (e.g. `(int64, string)` into `(int32, string)`)
//! or mismatched array lengths (e.g. `([4]int, string)` into `([3]int, string)`)
//! pass `hyb check` and fail `go build` (`cannot use ... as int32 value in assignment`).

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
fn multireturn_scalar_and_array_mismatches_reject() {
    for (name, src) in [
        (
            "multi-value assign int64 into int32",
            "fn get_pair() (int64, string) {\n    return 42, \"hello\"\n}\nfn main() {\n    var a int32\n    var b string\n    a, b = get_pair()\n    fmt.println(a, b)\n}\n",
        ),
        (
            "multi-value assign float64 into float32 and int32 into int64",
            "fn get_nums() (float64, int32) {\n    return 1.0, 2\n}\nfn main() {\n    var f float32\n    var i int64\n    f, i = get_nums()\n    fmt.println(f, i)\n}\n",
        ),
        (
            "method multi-value assign scalar mismatch",
            "struct worker {}\nfn (w worker) produce() (int64, bool) {\n    return 100, true\n}\nfn main() {\n    var w worker\n    var id int32\n    var ok bool\n    id, ok = w.produce()\n    fmt.println(id, ok)\n}\n",
        ),
        (
            "multi-value assign array length mismatch",
            "fn get_arr() ([4]int, string) {\n    var arr [4]int\n    return arr, \"ok\"\n}\nfn main() {\n    var a [3]int\n    var s string\n    a, s = get_arr()\n    fmt.println(a, s)\n}\n",
        ),
        (
            "multi-value assign with int32 from int64 call",
            "fn get_pair() (int64, int64) {\n    return 1, 2\n}\nfn main() {\n    var a int32\n    var b int32\n    a, b = get_pair()\n    fmt.println(a, b)\n}\n",
        ),
        (
            "comma-ok map index with int64 value into int32 variable",
            "fn main() {\n    var m map[string]int64\n    var v int32\n    var ok bool\n    v, ok = m[\"k\"]\n    fmt.println(v, ok)\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for {name}");
        let codes = error_codes(&r);
        assert!(
            codes.contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for {name}, got {codes:?}"
        );
    }
}

#[test]
fn multireturn_matching_types_accept() {
    for (name, src) in [
        (
            "matching int32 and string in multi-value assign",
            "fn get_pair() (int32, string) {\n    return 42, \"hello\"\n}\nfn main() {\n    var a int32\n    var b string\n    a, b = get_pair()\n    fmt.println(a, b)\n}\n",
        ),
        (
            "canonical rune alias into int32 multi-value assign",
            "fn get_pair() (rune, string) {\n    return 'a', \"hello\"\n}\nfn main() {\n    var a int32\n    var b string\n    a, b = get_pair()\n    fmt.println(a, b)\n}\n",
        ),
        (
            "matching int32 multi-value assign",
            "fn get_pair() (int32, int32) {\n    return 1, 2\n}\nfn main() {\n    var a int32\n    var b int32\n    a, b = get_pair()\n    fmt.println(a, b)\n}\n",
        ),
        (
            "matching comma-ok map index assign",
            "fn main() {\n    var m map[string]int32\n    var v int32\n    var ok bool\n    v, ok = m[\"k\"]\n    fmt.println(v, ok)\n}\n",
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
