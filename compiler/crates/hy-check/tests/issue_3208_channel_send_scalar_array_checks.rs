//! issue_3208_channel_send_scalar_array_checks.rs — regression tests for Issue #3208:
//! channel send scalar width and fixed-size array length validation.

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
fn test_issue_3208_channel_send_mismatches_rejected() {
    let cases = [
        // 1. scalar width mismatch: int64 to chan int32
        (
            "package main\nfn f() {\n    var ch = make(chan int32)\n    var v int64 = 1\n    ch <- v\n}\nfn main() {}\n",
            "send int64 to chan int32",
        ),
        // 2. scalar width mismatch: float64 to chan float32
        (
            "package main\nfn f() {\n    var ch = make(chan float32)\n    var v float64 = 1.5\n    ch <- v\n}\nfn main() {}\n",
            "send float64 to chan float32",
        ),
        // 3. scalar signedness mismatch: uint to chan int
        (
            "package main\nfn f() {\n    var ch = make(chan int)\n    var v uint = 1\n    ch <- v\n}\nfn main() {}\n",
            "send uint to chan int",
        ),
        // 4. fixed-size array variable length mismatch
        (
            "package main\nfn f() {\n    var ch = make(chan [2]int)\n    var a [3]int\n    ch <- move a\n}\nfn main() {}\n",
            "send [3]int to chan [2]int",
        ),
        // 5. fixed-size array literal length mismatch
        (
            "package main\nfn f() {\n    var ch = make(chan [2]int)\n    ch <- [3]int{1, 2, 3}\n}\nfn main() {}\n",
            "send array literal [3]int to chan [2]int",
        ),
    ];

    for (src, desc) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected reject for {desc}, got accept"
        );
        let codes = error_codes(&r);
        assert!(
            codes.contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for {desc}, got: {:?}",
            codes
        );
    }
}

#[test]
fn test_issue_3208_valid_channel_sends_accepted() {
    let cases = [
        // 1. matching scalar
        "package main\nfn f() {\n    var ch = make(chan int32)\n    var v int32 = 1\n    ch <- v\n}\nfn main() {}\n",
        // 2. matching array variable
        "package main\nfn f() {\n    var ch = make(chan [2]int)\n    var a [2]int\n    ch <- move a\n}\nfn main() {}\n",
        // 3. matching array literal
        "package main\nfn f() {\n    var ch = make(chan [2]int)\n    ch <- [2]int{1, 2}\n}\nfn main() {}\n",
        // 4. matching string
        "package main\nfn f() {\n    var ch = make(chan string)\n    ch <- \"hello\"\n}\nfn main() {}\n",
    ];

    for (i, src) in cases.iter().enumerate() {
        let r = check(src);
        let codes = error_codes(&r);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "case #{i} should accept cleanly, but got errors: {:?}",
            codes
        );
    }
}
