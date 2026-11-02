//! issue_3105_chan_for_in_and_select_scalar_widths.rs — issue #3105 (Row 13 BUILDABLE).
//!
//! Typed widths from channel for-in bindings and select receive arms escaped
//! the #2712 scalar width check because element scalars were not recorded on
//! the loop/arm bindings.

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
fn select_receive_arms_reject_scalar_mismatch_e1002() {
    let cases = [
        (
            "Row 1: select single-binding assign to int64",
            "fn main() {\n    c := make(chan int8, 1)\n    var out int64 = 0\n    select {\n        v := <-c: out = v\n    }\n    _ = out\n}\n",
        ),
        (
            "Row 2: select comma-ok assign to int64",
            "fn main() {\n    c := make(chan int8, 1)\n    var out int64 = 0\n    select {\n        v, ok := <-c: out = v\n    }\n    _ = out\n}\n",
        ),
        (
            "Row 3: select nested short-binding to int64 var",
            "fn main() {\n    c := make(chan int8, 1)\n    select {\n        v := <-c: {\n            x := v\n            var y int64 = x\n            _ = y\n        }\n    }\n}\n",
        ),
        (
            "Row 4: select binary op mismatch with conversion",
            "fn main() {\n    c := make(chan int8, 1)\n    var y int64 = 0\n    select {\n        v := <-c: y = int64(v) + v\n    }\n    _ = y\n}\n",
        ),
    ];

    for (name, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected reject for case: {name}\nsrc:\n{src}"
        );
        let codes = error_codes(&r);
        assert!(
            codes.contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 in {codes:?} for case: {name}"
        );
    }
}

#[test]
fn channel_for_in_bindings_reject_scalar_mismatch_e1002() {
    let cases = [
        (
            "Row 5: for v in chan int8 assigned to int64",
            "fn main() {\n    c := make(chan int8, 1)\n    var t int64 = 0\n    for v in c {\n        t = v\n    }\n    _ = t\n}\n",
        ),
        (
            "Row 6: for v in chan uint16 assigned to int var",
            "fn main() {\n    c := make(chan uint16, 1)\n    for v in c {\n        var y int = v\n        _ = y\n    }\n}\n",
        ),
    ];

    for (name, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected reject for case: {name}\nsrc:\n{src}"
        );
        let codes = error_codes(&r);
        assert!(
            codes.contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 in {codes:?} for case: {name}"
        );
    }
}

#[test]
fn channel_for_in_and_select_valid_controls_accept() {
    let cases = [
        (
            "matching int8 for-in addition",
            "fn main() {\n    c := make(chan int8, 1)\n    var s int8 = 0\n    for v in c {\n        s = s + v\n    }\n    _ = s\n}\n",
        ),
        (
            "select receive with matching int8 and explicit int64 cast",
            "fn main() {\n    d := make(chan int8, 1)\n    select {\n        v := <-d: {\n            var y int8 = v\n            var z int64 = int64(v)\n            _ = y\n            _ = z\n        }\n    }\n}\n",
        ),
    ];

    for (name, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "expected accept for valid case: {name}, got {r:?}\nsrc:\n{src}"
        );
    }
}
