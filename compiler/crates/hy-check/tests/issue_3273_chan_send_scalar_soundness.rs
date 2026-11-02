//! issue_3273_chan_send_scalar_soundness.rs — regression tests for channel
//! send element scalar width and fixed-size array dimension soundness under
//! Row 13 (BUILDABLE) and HYB-E1002 (issue #3273).

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
fn chan_send_mismatched_integer_scalar_rejected() {
    let src = r#"
package main

fn test_send() {
    var ch chan int64 = make(chan int64, 1)
    var v int32 = 42
    ch <- v
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "sending int32 to chan int64 must be rejected"
    );
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn chan_send_untyped_constant_accepted() {
    let src = r#"
package main

fn test_send_const() {
    var ch chan int64 = make(chan int64, 1)
    ch <- 42
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "sending untyped constant 42 to chan int64 must be accepted"
    );
}

#[test]
fn select_send_mismatched_integer_scalar_rejected() {
    let src = r#"
package main

fn test_select() {
    var ch chan int64 = make(chan int64, 1)
    var v int32 = 42
    select {
        ch <- v: {}
        default: {}
    }
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "sending int32 to chan int64 in select case must be rejected"
    );
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn chan_send_mismatched_array_dimension_rejected() {
    let src = r#"
package main

fn test_arr() {
    var ch chan [2]int = make(chan [2]int, 1)
    var a [3]int
    ch <- move a
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "sending [3]int to chan [2]int must be rejected"
    );
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn chan_send_mismatched_slice_element_scalar_rejected() {
    let src = r#"
package main

fn test_slice() {
    var ch chan []int64 = make(chan []int64, 1)
    var s []int32
    ch <- move s
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "sending []int32 to chan []int64 must be rejected"
    );
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn chan_send_alias_channel_mismatched_scalar_rejected() {
    let src = r#"
package main

type my_chan = chan int64

fn test_alias() {
    var ch my_chan = make(my_chan, 1)
    var v int32 = 100
    ch <- v
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "sending int32 to my_chan (chan int64) must be rejected"
    );
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn chan_send_inferred_channel_mismatched_scalar_rejected() {
    let src = r#"
package main

fn test_inferred() {
    ch := make(chan int64, 1)
    var v int32 = 55
    ch <- v
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "sending int32 to inferred chan int64 must be rejected"
    );
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn chan_send_conflicting_kind_rejected() {
    let src = r#"
package main

fn test_kind_conflict() {
    var ch chan string = make(chan string, 1)
    ch <- 123
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "sending integer to chan string must be rejected"
    );
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn chan_send_matching_types_accepted() {
    let src = r#"
package main

fn test_valid() {
    var ch chan int64 = make(chan int64, 1)
    var v int64 = 42
    ch <- v

    var sch chan []int64 = make(chan []int64, 1)
    var s []int64
    sch <- move s

    var ach chan [2]int = make(chan [2]int, 1)
    var a [2]int
    ach <- move a
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "matching channel sends must be accepted"
    );
    assert!(error_codes(&r).is_empty());
}
