//! issue_3218_fn_lit_map_lit_checks.rs — regression tests for Issue #3218:
//! scalar width, fixed-size array lengths, and multi-value call validation
//! in direct func literal calls and map composite literals.

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
fn test_issue_3218_fn_lit_and_map_lit_rejections() {
    let cases = [
        // 1. func literal scalar width mismatch (int64 -> int32)
        (
            "package main\nfn main() {\n    x := int64(10)\n    (fn(a int32) {\n        _ = a\n    })(x)\n}\n",
            "func literal argument scalar width mismatch int64 to int32",
        ),
        // 2. func literal float width mismatch (float64 -> float32)
        (
            "package main\nfn main() {\n    f := float64(1.5)\n    (fn(a float32) {\n        _ = a\n    })(f)\n}\n",
            "func literal argument float width mismatch float64 to float32",
        ),
        // 3. func literal typed int to float conflict
        (
            "package main\nfn main() {\n    i := int(5)\n    (fn(a float64) {\n        _ = a\n    })(i)\n}\n",
            "func literal typed int variable to float parameter",
        ),
        // 4. func literal fixed-size array length mismatch ([3]int -> [2]int)
        (
            "package main\nfn main() {\n    arr := [3]int{1, 2, 3}\n    (fn(a [2]int) {\n        _ = a\n    })(arr)\n}\n",
            "func literal argument array length mismatch [3]int to [2]int",
        ),
        // 5. func literal multi-value call scalar width mismatch
        (
            "package main\nfn two() (int64, string) {\n    return 1, \"ok\"\n}\nfn main() {\n    (fn(a int32, b string) {\n        _ = a\n        _ = b\n    })(two())\n}\n",
            "func literal multi-value call argument scalar width mismatch",
        ),
        // 6. func literal multi-value call array length mismatch
        (
            "package main\nfn two() ([3]int, int) {\n    var a [3]int\n    return a, 1\n}\nfn main() {\n    (fn(a [2]int, b int) {\n        _ = a\n        _ = b\n    })(two())\n}\n",
            "func literal multi-value call argument array length mismatch",
        ),
        // 7. map literal key array length mismatch with variable
        (
            "package main\nfn main() {\n    k := [3]int{1, 2, 3}\n    m := map[[2]int]string{k: \"ok\"}\n    _ = m\n}\n",
            "map literal key array length mismatch variable",
        ),
        // 8. map literal key array length mismatch with literal
        (
            "package main\nfn main() {\n    m := map[[2]int]string{[3]int{1, 2, 3}: \"ok\"}\n    _ = m\n}\n",
            "map literal key array length mismatch literal",
        ),
        // 9. map literal value array length mismatch with variable
        (
            "package main\nfn main() {\n    v := [3]int{1, 2, 3}\n    m := map[string][2]int{\"k\": v}\n    _ = m\n}\n",
            "map literal value array length mismatch variable",
        ),
        // 10. map literal value array length mismatch with literal
        (
            "package main\nfn main() {\n    m := map[string][2]int{\"k\": [3]int{1, 2, 3}}\n    _ = m\n}\n",
            "map literal value array length mismatch literal",
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
fn test_issue_3218_fn_lit_and_map_lit_acceptances() {
    let cases = [
        // 1. matching scalar in func literal call
        (
            "package main\nfn main() {\n    x := int32(10)\n    (fn(a int32) {\n        _ = a\n    })(x)\n}\n",
            "matching int32 argument in func literal call",
        ),
        // 2. matching array in func literal call
        (
            "package main\nfn main() {\n    arr := [2]int{1, 2}\n    (fn(a [2]int) {\n        _ = a\n    })(arr)\n}\n",
            "matching [2]int argument in func literal call",
        ),
        // 3. matching multi-value call in func literal call
        (
            "package main\nfn two() (int32, [2]int) {\n    var a [2]int\n    return 1, a\n}\nfn main() {\n    (fn(a int32, b [2]int) {\n        _ = a\n        _ = b\n    })(two())\n}\n",
            "matching multi-value call argument in func literal call",
        ),
        // 4. matching map literal with array keys and values
        (
            "package main\nfn main() {\n    k := [2]int{1, 2}\n    v := [2]int{3, 4}\n    m := map[[2]int][2]int{k: v, [2]int{5, 6}: [2]int{7, 8}}\n    _ = m\n}\n",
            "matching array keys and values in map literal",
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
