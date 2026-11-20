//! Issue #3416 — call arguments with mismatched composite element types
//! must be rejected before emitting Go that cannot build.

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
fn function_method_and_closure_calls_reject_composite_type_mismatches() {
    let src = "\
type arr32 = [3]int32
type arr64 = [3]int64
type arr3 = [3]int
type arr4 = [4]int

struct worker {}

fn consume_chan(ch chan int32) {}
fn consume_map(m map[string]int32) {}
fn consume_slice(s []int32) {}
fn consume_arr(a arr32) {}
fn consume_arr_len(a arr3) {}
fn consume_ptr(p *int32) {}

fn (w worker) consume_map_method(m map[string]int32) {}

fn main() {
    var bad_ch chan int64
    var bad_map map[string]int64
    var bad_key map[int]int32
    var bad_slice []int64
    var bad_arr arr64
    var bad_arr_len arr4
    var bad_ptr *int64
    var w worker

    consume_chan(bad_ch)
    consume_map(bad_map)
    consume_map(bad_key)
    consume_slice(bad_slice)
    consume_arr(bad_arr)
    consume_arr_len(bad_arr_len)
    consume_ptr(bad_ptr)
    w.consume_map_method(bad_map)
    _ = (fn(s []int32) int { return len(s) })(bad_slice)
}
";

    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected composite argument mismatches to reject, got: {:?}",
        r.diagnostics
    );
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got: {:?}",
        r.diagnostics
    );
    assert!(
        error_codes(&r).len() >= 9,
        "expected all nine call-site mismatches to be diagnosed, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn matching_composite_types_and_assignable_channel_directions_stay_accepted() {
    let src = "\
type arr32 = [3]int32
type arr32_alias = arr32

fn consume_chan(ch chan<- int32) {}
fn consume_map(m map[string]int32) {}
fn consume_slice(s []int32) {}
fn consume_arr(a arr32) {}
fn consume_ptr(p *int32) {}

fn main() {
    var ch chan int32
    var m map[string]int32
    var s []int32
    var a arr32_alias
    var i int32
    var p *int32

    consume_chan(ch)
    consume_map(m)
    consume_slice(s)
    consume_arr(a)
    consume_ptr(p)
    _ = i
}
";

    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected compatible arguments to remain accepted, got: {:?}",
        r.diagnostics
    );
    assert!(error_codes(&r).is_empty(), "{:?}", r.diagnostics);
}
