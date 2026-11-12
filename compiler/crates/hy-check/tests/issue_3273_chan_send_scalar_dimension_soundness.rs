//! issue_3273_chan_send_scalar_dimension_soundness.rs — regression test for #3273:
//! channel send statements (`Stmt::Send`) and select send cases (`SelectArmKind::Send`)
//! must verify element type scalar, array dimension, and slice element soundness (HYB-E1002).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn test_chan_send_scalar_mismatch_rejects_e1002() {
    let src = r#"
fn test_chan_send_scalar() {
    var ch chan int64 = make(chan int64, 1)
    var v int32 = 42
    ch <- v
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for int32 into chan int64");
    let codes = error_codes(&r);
    assert!(codes.iter().any(|c| c == "HYB-E1002"), "expected HYB-E1002, got {:?}", codes);
}

#[test]
fn test_inferred_chan_make_scalar_mismatch_rejects_e1002() {
    let src = r#"
fn test_inferred_make() {
    var ch = make(chan int32)
    var v int64 = 1
    ch <- v
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for int64 into inferred chan int32");
    let codes = error_codes(&r);
    assert!(codes.iter().any(|c| c == "HYB-E1002"), "expected HYB-E1002, got {:?}", codes);
}

#[test]
fn test_select_send_scalar_mismatch_rejects_e1002() {
    let src = r#"
fn test_select_send_scalar() {
    var ch chan int64 = make(chan int64, 1)
    var v int32 = 42
    select {
        ch <- v: {}
        default: {}
    }
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for select send int32 into chan int64");
    let codes = error_codes(&r);
    assert!(codes.iter().any(|c| c == "HYB-E1002"), "expected HYB-E1002, got {:?}", codes);
}

#[test]
fn test_send_array_dimension_mismatch_rejects_e1002() {
    let src = r#"
fn test_array_dim() {
    var ch chan [2]int = make(chan [2]int, 1)
    var a [3]int
    ch <- move a
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for [3]int into chan [2]int");
    let codes = error_codes(&r);
    assert!(codes.iter().any(|c| c == "HYB-E1002"), "expected HYB-E1002, got {:?}", codes);
}

#[test]
fn test_send_array_lit_dimension_mismatch_rejects_e1002() {
    let src = r#"
fn test_array_lit_dim() {
    var ch chan [2]int = make(chan [2]int, 1)
    ch <- [3]int{1, 2, 3}
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for [3]int literal into chan [2]int");
    let codes = error_codes(&r);
    assert!(codes.iter().any(|c| c == "HYB-E1002"), "expected HYB-E1002, got {:?}", codes);
}

#[test]
fn test_send_array_element_scalar_mismatch_rejects_e1002() {
    let src = r#"
fn test_array_scalar() {
    var ch chan [2]int64 = make(chan [2]int64, 1)
    var a [2]int32
    ch <- move a
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for [2]int32 into chan [2]int64");
    let codes = error_codes(&r);
    assert!(codes.iter().any(|c| c == "HYB-E1002"), "expected HYB-E1002, got {:?}", codes);
}

#[test]
fn test_send_slice_element_scalar_mismatch_rejects_e1002() {
    let src = r#"
fn test_slice_scalar() {
    var ch chan []int64 = make(chan []int64, 1)
    var s []int32
    ch <- move s
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for []int32 into chan []int64");
    let codes = error_codes(&r);
    assert!(codes.iter().any(|c| c == "HYB-E1002"), "expected HYB-E1002, got {:?}", codes);
}

#[test]
fn test_select_send_slice_element_scalar_mismatch_rejects_e1002() {
    let src = r#"
fn test_select_slice_scalar() {
    var ch chan []int64 = make(chan []int64, 1)
    var s []int32
    select {
        ch <- move s: {}
        default: {}
    }
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for select send []int32 into chan []int64");
    let codes = error_codes(&r);
    assert!(codes.iter().any(|c| c == "HYB-E1002"), "expected HYB-E1002, got {:?}", codes);
}

#[test]
fn test_send_type_alias_channel_scalar_mismatch_rejects_e1002() {
    let src = r#"
type int_chan = chan int64

fn test_alias() {
    var ch int_chan = make(int_chan, 1)
    var v int32 = 42
    ch <- v
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for int32 into int_chan");
    let codes = error_codes(&r);
    assert!(codes.iter().any(|c| c == "HYB-E1002"), "expected HYB-E1002, got {:?}", codes);
}

#[test]
fn test_send_matching_scalar_and_untyped_accepts() {
    let src = r#"
fn test_matching_scalar() {
    var ch chan int64 = make(chan int64, 1)
    var v int64 = 42
    ch <- v
    ch <- 100
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept for matching scalar and untyped int, got {:?}", r.diagnostics);
}

#[test]
fn test_send_matching_array_and_slice_accepts() {
    let src = r#"
fn test_matching_composites() {
    var ch_arr chan [2]int = make(chan [2]int, 1)
    var a [2]int
    ch_arr <- move a

    var ch_slice chan []int32 = make(chan []int32, 1)
    var s []int32
    ch_slice <- move s
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept for matching array and slice, got {:?}", r.diagnostics);
}
