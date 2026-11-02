//! Regression tests for issue #3172:
//! Channel send statements (`ch <- val`) and receive expressions (`<-ch`)
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
fn issue_3172_rejects_chan_send_and_recv_scalar_mismatches() {
    let cases = [
        // 1. Send statement scalar mismatch (int32 vs int64)
        (
            "fn main() {\n    var ch chan int32\n    var x int64 = 42\n    ch <- x\n}\n",
            "send int64 into chan int32",
        ),
        // 2. Send statement scalar mismatch (float32 vs float64)
        (
            "fn main() {\n    var ch chan float32\n    var f float64 = 1.5\n    ch <- f\n}\n",
            "send float64 into chan float32",
        ),
        // 3. Make channel send scalar mismatch
        (
            "fn main() {\n    ch := make(chan int32, 1)\n    var x int64 = 42\n    ch <- x\n}\n",
            "send int64 into make(chan int32)",
        ),
        // 4. Select arm send scalar mismatch
        (
            "fn main() {\n    var ch chan int32\n    var x int64 = 42\n    select {\n        ch <- x:\n    }\n}\n",
            "select arm send int64 into chan int32",
        ),
        // 5. Array length mismatch on send
        (
            "fn main() {\n    var ch chan [2]int\n    ch <- [3]int{1, 2, 3}\n}\n",
            "send [3]int into chan [2]int",
        ),
        // 6. Receive expression in var declaration
        (
            "fn main() {\n    var ch chan int32\n    var y int64 = <-ch\n    _ = y\n}\n",
            "initialize int64 with receive from chan int32",
        ),
        // 7. Receive expression in assignment
        (
            "fn main() {\n    var ch chan int32\n    var y int64\n    y = <-ch\n    _ = y\n}\n",
            "assign receive from chan int32 to int64 target",
        ),
        // 8. Receive array length mismatch in assignment
        (
            "fn main() {\n    var ch chan [2]int\n    var a [3]int\n    a = <-ch\n    _ = a\n}\n",
            "assign receive from chan [2]int to [3]int target",
        ),
        // 9. Comma-ok receive assignment scalar mismatch
        (
            "fn main() {\n    var ch chan int32\n    var val int64\n    var ok bool\n    val, ok = <-ch\n    _ = val\n    _ = ok\n}\n",
            "comma-ok receive into int64 from chan int32",
        ),
        // 10. Short define receive propagates scalar and catches downstream conflict
        (
            "fn main() {\n    ch := make(chan int32, 1)\n    y := <-ch\n    var z int64\n    z = y\n    _ = z\n}\n",
            "short define receive propagates int32 scalar to downstream assignment",
        ),
        // 11. Select arm receive propagates scalar to body assignment
        (
            "fn main() {\n    var ch chan int32\n    var z int64\n    select {\n        val := <-ch:\n            z = val\n            _ = z\n    }\n}\n",
            "select arm receive propagates int32 scalar to body assignment",
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
fn issue_3172_accepts_valid_chan_send_and_recv() {
    let cases = [
        // 1. Same-type scalar send
        "fn main() {\n    var ch chan int32\n    var x int32 = 42\n    ch <- x\n}\n",
        // 2. Untyped int literal send
        "fn main() {\n    var ch chan int32\n    ch <- 42\n}\n",
        // 3. Untyped float literal send
        "fn main() {\n    var ch chan float32\n    ch <- 1.5\n}\n",
        // 4. Same-type scalar receive initialization
        "fn main() {\n    var ch chan int32\n    var y int32 = <-ch\n    _ = y\n}\n",
        // 5. Select arm send and receive controls
        "fn main() {\n    var ch chan int32\n    var x int32 = 42\n    select {\n        ch <- x:\n        val := <-ch:\n            _ = val\n    }\n}\n",
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
