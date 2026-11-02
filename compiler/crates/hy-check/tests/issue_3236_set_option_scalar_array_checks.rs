//! issue_3236_set_option_scalar_array_checks.rs — regression tests for Issue #3236:
//! scalar width and fixed-size array length matching in built-in methods on `set[T]` and `option[T]`.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("snippet must parse");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.to_string())
        .collect()
}

#[test]
fn test_issue_3236_set_and_option_rejections() {
    let cases = [
        // 1. set insert scalar width mismatch (int64 -> int32)
        (
            "package main\nfn main() {\n    x := int64(1)\n    s := set[int32]{}\n    s.insert(x)\n}\n",
            "set insert scalar width mismatch int64 to int32",
        ),
        // 2. set contains float width mismatch (float64 -> float32)
        (
            "package main\nfn main() {\n    f := float64(1.0)\n    s := set[float32]{}\n    _ = s.contains(f)\n}\n",
            "set contains float width mismatch float64 to float32",
        ),
        // 3. set insert array length mismatch ([3]int -> [2]int)
        (
            "package main\nfn main() {\n    arr3 := [3]int{1, 2, 3}\n    s := set[[2]int]{}\n    s.insert(arr3)\n}\n",
            "set insert array length mismatch [3]int to [2]int",
        ),
        // 4. option.or scalar width mismatch (int64 -> int32)
        (
            "package main\nfn main() {\n    x := int64(1)\n    opt := some(int32(10))\n    _ = opt.or(x)\n}\n",
            "option.or scalar width mismatch int64 to int32",
        ),
        // 5. option.or array length mismatch ([3]int -> [2]int)
        (
            "package main\nfn main() {\n    arr3 := [3]int{1, 2, 3}\n    opt := some([2]int{1, 2})\n    _ = opt.or(arr3)\n}\n",
            "option.or array length mismatch [3]int to [2]int",
        ),
        // 6. inline some.or array length mismatch with literal
        (
            "package main\nfn main() {\n    _ = some([2]int{1, 2}).or([3]int{1, 2, 3})\n}\n",
            "inline some.or array length mismatch with literal",
        ),
    ];

    for (src, label) in cases {
        let res = check(src);
        let codes = error_codes(&res);
        assert_eq!(
            res.verdict,
            Verdict::Reject,
            "{label}: expected Verdict::Reject, got {:?}, diagnostics: {:?}",
            res.verdict,
            res.diagnostics
        );
        assert!(
            codes.iter().any(|c| c == "HYB-E1002"),
            "{label}: expected HYB-E1002 in {:?}, diagnostics: {:?}",
            codes,
            res.diagnostics
        );
    }
}

#[test]
fn test_issue_3236_set_and_option_acceptances() {
    let cases = [
        // 1. matching scalar in set insert and contains
        (
            "package main\nfn main() {\n    x := int32(1)\n    s := set[int32]{}\n    s.insert(x)\n    _ = s.contains(x)\n    s.remove(x)\n}\n",
            "matching int32 in set operations",
        ),
        // 2. matching array variable in set insert
        (
            "package main\nfn main() {\n    arr2 := [2]int{1, 2}\n    s := set[[2]int]{}\n    s.insert(arr2)\n    _ = s.contains(arr2)\n}\n",
            "matching array variable in set operations",
        ),
        // 3. matching scalar in option.or
        (
            "package main\nfn main() {\n    x := int32(1)\n    opt := some(int32(10))\n    _ = opt.or(x)\n}\n",
            "matching scalar in option.or",
        ),
        // 4. matching array in option.or
        (
            "package main\nfn main() {\n    arr2 := [2]int{1, 2}\n    opt := some([2]int{1, 2})\n    _ = opt.or(arr2)\n}\n",
            "matching array in option.or",
        ),
    ];

    for (src, label) in cases {
        let res = check(src);
        assert_eq!(
            res.verdict,
            Verdict::Accept,
            "{label}: expected Verdict::Accept, got {:?}, diagnostics: {:?}",
            res.verdict,
            res.diagnostics
        );
        assert!(
            res.diagnostics.is_empty(),
            "{label}: expected 0 diagnostics, got: {:?}",
            res.diagnostics
        );
    }
}
