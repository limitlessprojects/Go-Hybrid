//! Regression tests for issue #3180:
//! Function literal calls, multi-value argument expansion, and return statements
//! must enforce fixed-width scalar widths and array lengths under HYB-E1002.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn error_codes(res: &hy_check::CheckResult) -> Vec<String> {
    res.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.to_string())
        .collect()
}

#[test]
fn issue_3180_rejects_call_and_return_mismatches() {
    let cases = [
        // 1. Direct function literal call with integer scalar mismatch (int32 vs int64)
        (
            "fn main() {\n    var x int64 = 42\n    fn(a int32) {}(x)\n}\n",
            "fn lit call with int64 passed to int32 parameter",
        ),
        // 2. Direct function literal call with float scalar mismatch (float32 vs float64)
        (
            "fn main() {\n    var f float64 = 3.14\n    fn(a float32) {}(f)\n}\n",
            "fn lit call with float64 passed to float32 parameter",
        ),
        // 3. Direct function literal call with array length mismatch
        (
            "fn main() {\n    var arr [3]int\n    fn(a [2]int) {}(arr)\n}\n",
            "fn lit call with [3]int passed to [2]int parameter",
        ),
        // 4. Direct function literal call with array literal mismatch
        (
            "fn main() {\n    fn(a [2]int) {}([3]int{1, 2, 3})\n}\n",
            "fn lit call with [3]int literal passed to [2]int parameter",
        ),
        // 5. Multi-value call argument scalar mismatch
        (
            "fn pair() (int64, string) {\n    return 1, \"ok\"\n}\nfn foo(a int32, b string) {}\nfn main() {\n    foo(pair())\n}\n",
            "multi-value argument expansion with int64 passed to int32 parameter",
        ),
        // 6. Return statement array length mismatch from local variable
        (
            "fn get_arr() [2]int {\n    var a [3]int\n    return a\n}\nfn main() {}\n",
            "return [3]int variable from function declaring [2]int",
        ),
        // 7. Return statement array literal length mismatch
        (
            "fn get_arr_lit() [2]int {\n    return [3]int{1, 2, 3}\n}\nfn main() {}\n",
            "return [3]int literal from function declaring [2]int",
        ),
        // 8. Closure return statement array length mismatch
        (
            "fn main() {\n    f := fn() [2]int {\n        var a [3]int\n        return a\n    }\n    _ = f\n}\n",
            "closure returning [3]int when declared to return [2]int",
        ),
    ];

    for (src, label) in cases {
        let file = parse_file("test.hy", src).unwrap();
        let res = check_file(&file);
        assert_eq!(
            res.verdict,
            Verdict::Reject,
            "expected reject for case '{}':\n{}",
            label,
            src
        );
        let codes = error_codes(&res);
        assert!(
            codes.iter().any(|c| c == "HYB-E1002"),
            "expected HYB-E1002 for case '{}', got {:?}:\n{}",
            label,
            codes,
            src
        );
    }
}

#[test]
fn issue_3180_accepts_valid_call_and_return_cases() {
    let cases = [
        // 1. Function literal call with matching scalar
        "fn main() {\n    var x int32 = 42\n    fn(a int32) {}(x)\n}\n",
        // 2. Function literal call with untyped constant
        "fn main() {\n    fn(a int32) {}(42)\n}\n",
        // 3. Function literal call with matching array
        "fn main() {\n    var arr [2]int\n    fn(a [2]int) {}(arr)\n}\n",
        // 4. Multi-value call argument matching types
        "fn pair() (int32, string) {\n    return 1, \"ok\"\n}\nfn foo(a int32, b string) {}\nfn main() {\n    foo(pair())\n}\n",
        // 5. Correct array return
        "fn get_arr() [2]int {\n    var a [2]int\n    return a\n}\nfn main() {}\n",
        // 6. Correct array literal return
        "fn get_arr_lit() [2]int {\n    return [2]int{1, 2}\n}\nfn main() {}\n",
        // 7. Correct closure array return
        "fn main() {\n    f := fn() [2]int {\n        var a [2]int\n        return a\n    }\n    _ = f\n}\n",
    ];

    for src in cases {
        let file = parse_file("test.hy", src).unwrap();
        let res = check_file(&file);
        assert_eq!(
            res.verdict,
            Verdict::Accept,
            "unexpected diagnostics for:\n{}\n{:?}",
            src,
            res.diagnostics
        );
    }
}
