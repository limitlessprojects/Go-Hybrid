//! issue_3208_chan_send_scalar_array.rs — regression test for #3208:
//! channel send statements (`ch <- val`) must reject scalar width mismatches
//! and fixed-size array length mismatches with HYB-E1002 (Row 13 BUILDABLE).

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
fn test_send_scalar_mismatch_int64_into_int32_rejects() {
    let src = r#"
fn test() {
    var ch = make(chan int32)
    var v int64 = 1
    ch <- v
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for int64 into chan int32");
    let codes = error_codes(&r);
    assert!(codes.iter().any(|c| c == "HYB-E1002"), "expected HYB-E1002, got {:?}", codes);
}

#[test]
fn test_send_scalar_mismatch_int_into_int16_rejects() {
    let src = r#"
fn test() {
    var ch = make(chan int16)
    var v int = 1
    ch <- v
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for int into chan int16");
    let codes = error_codes(&r);
    assert!(codes.iter().any(|c| c == "HYB-E1002"), "expected HYB-E1002, got {:?}", codes);
}

#[test]
fn test_send_scalar_mismatch_float64_into_float32_rejects() {
    let src = r#"
fn test() {
    var ch = make(chan float32)
    var v float64 = 1.0
    ch <- v
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for float64 into chan float32");
    let codes = error_codes(&r);
    assert!(codes.iter().any(|c| c == "HYB-E1002"), "expected HYB-E1002, got {:?}", codes);
}

#[test]
fn test_send_array_len_mismatch_var_rejects() {
    let src = r#"
fn test() {
    var ch = make(chan [2]int)
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
fn test_send_array_len_mismatch_lit_rejects() {
    let src = r#"
fn test() {
    var ch = make(chan [2]int)
    ch <- [3]int{1, 2, 3}
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for [3]int literal into chan [2]int");
    let codes = error_codes(&r);
    assert!(codes.iter().any(|c| c == "HYB-E1002"), "expected HYB-E1002, got {:?}", codes);
}

#[test]
fn test_send_array_len_mismatch_decl_chan_rejects() {
    let src = r#"
fn test() {
    var ch chan [2]int
    var a [4]int
    ch <- move a
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for [4]int into declared chan [2]int");
    let codes = error_codes(&r);
    assert!(codes.iter().any(|c| c == "HYB-E1002"), "expected HYB-E1002, got {:?}", codes);
}

#[test]
fn test_select_arm_send_scalar_mismatch_rejects() {
    let src = r#"
fn test() {
    var ch = make(chan int32)
    var v int64 = 1
    select {
    ch <- v:
        println("sent")
    default:
        println("default")
    }
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for select arm int64 into chan int32");
    let codes = error_codes(&r);
    assert!(codes.iter().any(|c| c == "HYB-E1002"), "expected HYB-E1002, got {:?}", codes);
}

#[test]
fn test_send_matching_scalar_accepts() {
    let src = r#"
fn test() {
    var ch = make(chan int32)
    var v int32 = 1
    ch <- v
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept for matching int32 into chan int32, got {:?}", r.diagnostics);
}

#[test]
fn test_send_matching_array_accepts() {
    let src = r#"
fn test() {
    var ch = make(chan [2]int)
    var a [2]int
    ch <- move a
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept for matching [2]int into chan [2]int, got {:?}", r.diagnostics);
}

#[test]
fn test_send_untyped_int_const_accepts() {
    let src = r#"
fn test() {
    var ch = make(chan int32)
    ch <- 10
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept for untyped literal into chan int32, got {:?}", r.diagnostics);
}
