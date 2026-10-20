//! Regression tests for issue #1044: HYB-E3011 channel element type checks
//! across composite inner shapes (slices, arrays, maps), slice vs fixed-array
//! channel elements, and primitive kind mismatches (`bool` vs `int`/`float64`).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
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
fn send_bool_into_int_channel_rejects_e3011() {
    let src = "\
fn main() {
    ch := make(chan int, 1)
    ch <- true
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E3011"),
        "expected HYB-E3011, got {:?}",
        r.diagnostics
    );
}

#[test]
fn send_int_into_bool_channel_rejects_e3011() {
    let src = "\
fn main() {
    ch := make(chan bool, 1)
    ch <- 42
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E3011"),
        "expected HYB-E3011, got {:?}",
        r.diagnostics
    );
}

#[test]
fn send_typed_float_into_int_channel_rejects_e3011() {
    let src = "\
fn main() {
    ch := make(chan int, 1)
    var f float64 = 1.5
    ch <- f
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E3011"),
        "expected HYB-E3011, got {:?}",
        r.diagnostics
    );
}

#[test]
fn send_mismatched_slice_element_into_slice_channel_rejects_e3011() {
    let src = "\
fn main() {
    ch := make(chan []int, 1)
    xs := []string{\"a\"}
    ch <- move xs
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E3011"),
        "expected HYB-E3011, got {:?}",
        r.diagnostics
    );
}

#[test]
fn send_mismatched_map_value_into_map_channel_rejects_e3011() {
    let src = "\
fn main() {
    ch := make(chan map[string]int, 1)
    m := map[string]string{\"a\": \"b\"}
    ch <- move m
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E3011"),
        "expected HYB-E3011, got {:?}",
        r.diagnostics
    );
}

#[test]
fn send_slice_into_array_channel_and_recv_array_into_slice_reject_e3011() {
    let send_src = "\
fn main() {
    ch := make(chan [2]int, 1)
    xs := []int{1, 2}
    ch <- move xs
}
";
    let r_send = check(send_src);
    assert_eq!(r_send.verdict, Verdict::Reject);
    assert!(
        error_codes(&r_send).iter().any(|c| c == "HYB-E3011"),
        "expected HYB-E3011 on slice send to array chan, got {:?}",
        r_send.diagnostics
    );

    let recv_src = "\
fn main() {
    ch := make(chan [2]int, 1)
    ch <- [2]int{1, 2}
    var xs []int = <-ch
    fmt.println(\"{len(xs)}\")
}
";
    let r_recv = check(recv_src);
    assert_eq!(r_recv.verdict, Verdict::Reject);
    assert!(
        error_codes(&r_recv).iter().any(|c| c == "HYB-E3011"),
        "expected HYB-E3011 on array recv into slice target, got {:?}",
        r_recv.diagnostics
    );
}

#[test]
fn matching_composite_and_primitive_sends_accept() {
    let src = "\
fn main() {
    ch_b := make(chan bool, 1)
    ch_b <- true
    b := <-ch_b

    ch_f := make(chan float64, 1)
    ch_f <- 7
    f := <-ch_f

    ch_s := make(chan []int, 1)
    xs := []int{1, 2}
    ch_s <- move xs
    var got_s []int = <-ch_s

    fmt.println(\"{b} {f} {len(got_s)}\")
}
";
    let r = check(src);
    assert_eq!(
        error_codes(&r),
        Vec::<String>::new(),
        "matching sends/receives must stay clean, got {:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Accept);
}
