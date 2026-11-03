//! issue_3412_composite_literal_container_soundness.rs
//! Regression tests for Issue #3412 (Row 13 BUILDABLE):
//! Slice, array, and map composite literals must reject mismatched container
//! element, key, and value types (channels, maps, slices, sized arrays, and pointers).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src).unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_rejects_e1002(src: &str, what: &str) {
    let r = check(src);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {}, got verdict {:?}, codes {:?}",
        what,
        r.verdict,
        codes
    );
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for {}", what);
}

fn assert_accept(src: &str, what: &str) {
    let r = check(src);
    let codes = error_codes(&r);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected Accept for {}, got errors: {:?}",
        what,
        codes
    );
}

#[test]
fn slice_literal_container_element_mismatch_rejected() {
    let src1 = "fn sink[t any](x t) {}\nfn main() {\n    var bad_chan chan int64\n    s := []chan int32{ bad_chan }\n    sink(s)\n}\n";
    assert_rejects_e1002(src1, "slice literal chan int32 with chan int64 element");

    let src2 = "fn sink[t any](x t) {}\nfn main() {\n    var bad_arr [4]int\n    s := [][3]int{ bad_arr }\n    sink(s)\n}\n";
    assert_rejects_e1002(src2, "slice literal [3]int with [4]int element");

    let src3 = "fn sink[t any](x t) {}\nfn main() {\n    var bad_slice []int64\n    s := [][]int32{ bad_slice }\n    sink(s)\n}\n";
    assert_rejects_e1002(src3, "slice literal []int32 with []int64 element");

    let src4 = "fn sink[t any](x t) {}\nfn main() {\n    var bad_map map[string]int64\n    s := []map[string]int32{ bad_map }\n    sink(s)\n}\n";
    assert_rejects_e1002(src4, "slice literal map[string]int32 with map[string]int64 element");

    let src5 = "fn sink[t any](x t) {}\nfn main() {\n    var bad_ptr *int64\n    s := []*int32{ bad_ptr }\n    sink(s)\n}\n";
    assert_rejects_e1002(src5, "slice literal *int32 with *int64 element");
}

#[test]
fn array_literal_container_element_mismatch_rejected() {
    let src1 = "fn sink[t any](x t) {}\nfn main() {\n    var bad_chan chan int64\n    a := [1]chan int32{ bad_chan }\n    sink(a)\n}\n";
    assert_rejects_e1002(src1, "array literal [1]chan int32 with chan int64 element");

    let src2 = "fn sink[t any](x t) {}\nfn main() {\n    var bad_arr [4]int\n    a := [1][3]int{ bad_arr }\n    sink(a)\n}\n";
    assert_rejects_e1002(src2, "array literal [1][3]int with [4]int element");

    let src3 = "fn sink[t any](x t) {}\nfn main() {\n    var bad_slice []int64\n    a := [1][]int32{ bad_slice }\n    sink(a)\n}\n";
    assert_rejects_e1002(src3, "array literal [1][]int32 with []int64 element");

    let src4 = "fn sink[t any](x t) {}\nfn main() {\n    var bad_map map[string]int64\n    a := [1]map[string]int32{ bad_map }\n    sink(a)\n}\n";
    assert_rejects_e1002(src4, "array literal [1]map[string]int32 with map[string]int64 element");

    let src5 = "fn sink[t any](x t) {}\nfn main() {\n    var bad_ptr *int64\n    a := [1]*int32{ bad_ptr }\n    sink(a)\n}\n";
    assert_rejects_e1002(src5, "array literal [1]*int32 with *int64 element");
}

#[test]
fn map_literal_container_key_and_value_mismatch_rejected() {
    let src1 = "fn sink[t any](x t) {}\nfn main() {\n    var bad_chan chan int64\n    m := map[string]chan int32{ \"k\": bad_chan }\n    sink(m)\n}\n";
    assert_rejects_e1002(src1, "map value chan int32 with chan int64");

    let src2 = "fn sink[t any](x t) {}\nfn main() {\n    var bad_chan chan int64\n    m := map[chan int32]string{ bad_chan: \"v\" }\n    sink(m)\n}\n";
    assert_rejects_e1002(src2, "map key chan int32 with chan int64");

    let src3 = "fn sink[t any](x t) {}\nfn main() {\n    var bad_arr [4]int\n    m := map[string][3]int{ \"k\": bad_arr }\n    sink(m)\n}\n";
    assert_rejects_e1002(src3, "map value [3]int with [4]int");

    let src4 = "fn sink[t any](x t) {}\nfn main() {\n    var bad_arr [4]int\n    m := map[[3]int]string{ bad_arr: \"v\" }\n    sink(m)\n}\n";
    assert_rejects_e1002(src4, "map key [3]int with [4]int");

    let src5 = "fn sink[t any](x t) {}\nfn main() {\n    var bad_ptr *int64\n    m := map[string]*int32{ \"k\": bad_ptr }\n    sink(m)\n}\n";
    assert_rejects_e1002(src5, "map value *int32 with *int64");

    let src6 = "fn sink[t any](x t) {}\nfn main() {\n    var bad_ptr *int64\n    m := map[*int32]string{ bad_ptr: \"v\" }\n    sink(m)\n}\n";
    assert_rejects_e1002(src6, "map key *int32 with *int64");

    let src7 = "fn sink[t any](x t) {}\nfn main() {\n    var bad_slice []int64\n    m := map[string][]int32{ \"k\": bad_slice }\n    sink(m)\n}\n";
    assert_rejects_e1002(src7, "map value []int32 with []int64");

    let src8 = "fn sink[t any](x t) {}\nfn main() {\n    var bad_map map[string]int64\n    m := map[string]map[string]int32{ \"k\": bad_map }\n    sink(m)\n}\n";
    assert_rejects_e1002(src8, "map value map[string]int32 with map[string]int64");
}

#[test]
fn type_aliases_container_element_mismatch_rejected() {
    let src1 = "type int_chan = chan int32\ntype long_chan = chan int64\nfn sink[t any](x t) {}\nfn main() {\n    var lc long_chan\n    s := []int_chan{ lc }\n    sink(s)\n}\n";
    assert_rejects_e1002(src1, "slice literal int_chan with long_chan alias element");

    let src2 = "type arr_3 = [3]int\ntype arr_4 = [4]int\nfn sink[t any](x t) {}\nfn main() {\n    var a4 arr_4\n    a := [1]arr_3{ a4 }\n    sink(a)\n}\n";
    assert_rejects_e1002(src2, "array literal arr_3 with arr_4 alias element");

    let src3 = "type arr_3 = [3]int\ntype arr_4 = [4]int\nfn sink[t any](x t) {}\nfn main() {\n    var a4 arr_4\n    m := map[arr_3]string{ a4: \"bad\" }\n    sink(m)\n}\n";
    assert_rejects_e1002(src3, "map literal key arr_3 with arr_4 alias");
}

#[test]
fn matching_container_composite_literals_accepted() {
    let src = "fn sink[t any](x t) {}\nfn main() {\n    var ok_chan chan int32\n    _ = []chan int32{ ok_chan }\n    _ = [1]chan int32{ ok_chan }\n    _ = map[string]chan int32{ \"k\": ok_chan }\n    _ = map[chan int32]string{ ok_chan: \"v\" }\n\n    var ok_arr [3]int\n    _ = [][3]int{ ok_arr }\n    _ = [1][3]int{ ok_arr }\n    _ = map[string][3]int{ \"k\": ok_arr }\n    _ = map[[3]int]string{ ok_arr: \"v\" }\n\n    var ok_slice []int32\n    _ = [][]int32{ ok_slice }\n    _ = [1][]int32{ ok_slice }\n    _ = map[string][]int32{ \"k\": ok_slice }\n\n    var ok_map map[string]int32\n    _ = []map[string]int32{ ok_map }\n    _ = [1]map[string]int32{ ok_map }\n    _ = map[string]map[string]int32{ \"k\": ok_map }\n\n    var ok_ptr *int32\n    _ = []*int32{ ok_ptr }\n    _ = [1]*int32{ ok_ptr }\n    _ = map[string]*int32{ \"k\": ok_ptr }\n    _ = map[*int32]string{ ok_ptr: \"v\" }\n\n    sink(1)\n}\n";
    assert_accept(src, "matching container composite literals");
}
