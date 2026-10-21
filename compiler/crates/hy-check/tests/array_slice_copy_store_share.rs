//! array_slice_copy_store_share.rs — a slice of an addressable primitive
//! array that reaches another binding through `copy` or a helper's
//! out-parameter store still views the array's storage.
//!
//! pr-319 (#318) taught `record_may_share` that `s := arr[:]` shares
//! `arr`'s backing storage even though `[N]int` alone never
//! `shape_may_share`s. Two sibling edge recorders kept the old
//! `binding_may_share`-only test on the source root and dropped the edge:
//!
//! * `record_copy_may_share` — `copy(dst, [][]int{arr[:]})` copies a slice
//!   header that views `arr` into `dst[0]`;
//! * `record_callee_param_stores` — `store_slice(dst, arr[:])` where the
//!   helper does `dst[0] = s`.
//!
//! In both cases `task worker(move dst, done)` froze only `dst`, the
//! parent kept writing `arr[0]`, `hyb check` reported 0 diagnostics, and
//! `go run -race` on the emitted Go reported `WARNING: DATA RACE`.
//!
//! False-positive guards: a by-value array copy (`clone4(arr)[:]`), an
//! element-wise copy of primitives (`copy(ints, arr[:])`), a helper that
//! never stores its argument, and no concurrency boundary all stay
//! accepted.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

const PRELUDE: &str = "struct arr_box {\n\
         data [4]int\n\
     }\n\
     fn worker(rows [][]int, done chan<- bool) {\n\
         rows[0][0] = 1\n\
         done <- true\n\
     }\n\
     fn worker_ints(s []int, done chan<- bool) {\n\
         s[0] = 1\n\
         done <- true\n\
     }\n\
     fn store_slice(dst [][]int, s []int) {\n\
         dst[0] = s\n\
     }\n\
     fn peek_slice(dst [][]int, s []int) {\n\
         _ = len(dst) + len(s)\n\
     }\n\
     fn clone4(a [4]int) [4]int {\n\
         return a\n\
     }\n";

fn program(main_body: &str) -> String {
    format!("{}fn main() {{\n{}\n}}\n", PRELUDE, main_body)
}

fn assert_e3003(body: &str) {
    let r = check(&program(body));
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        codes(&r).iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 for the write through the array after moving a container \
         holding its slice, got {:?}",
        r.diagnostics
    );
}

fn assert_accept(body: &str) {
    let r = check(&program(body));
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn copy_of_array_slice_into_container_freezes_the_array() {
    assert_e3003(
        "    done := make(chan bool, 1)\n\
             arr := [4]int{1, 2, 3, 4}\n\
             dst := [][]int{[]int{0}}\n\
             copy(dst, [][]int{arr[:]})\n\
             task worker(move dst, done)\n\
             arr[0] = 2\n\
             <-done",
    );
}

#[test]
fn copy_of_struct_array_field_slice_freezes_the_struct() {
    assert_e3003(
        "    done := make(chan bool, 1)\n\
             b := arr_box{data: [4]int{1, 2, 3, 4}}\n\
             dst := [][]int{[]int{0}}\n\
             copy(dst, [][]int{b.data[:]})\n\
             task worker(move dst, done)\n\
             b.data[0] = 2\n\
             <-done",
    );
}

#[test]
fn copy_of_bounded_array_reslice_freezes_the_array() {
    assert_e3003(
        "    done := make(chan bool, 1)\n\
             var arr [4]int\n\
             dst := make([][]int, 1)\n\
             copy(dst, [][]int{arr[1:3]})\n\
             task worker(move dst, done)\n\
             arr[1] = 2\n\
             <-done",
    );
}

#[test]
fn helper_store_of_array_slice_freezes_the_array() {
    assert_e3003(
        "    done := make(chan bool, 1)\n\
             arr := [4]int{1, 2, 3, 4}\n\
             dst := [][]int{[]int{0}}\n\
             store_slice(dst, arr[:])\n\
             task worker(move dst, done)\n\
             arr[0] = 2\n\
             <-done",
    );
}

#[test]
fn helper_store_of_struct_array_field_slice_freezes_the_struct() {
    assert_e3003(
        "    done := make(chan bool, 1)\n\
             b := arr_box{data: [4]int{1, 2, 3, 4}}\n\
             dst := [][]int{[]int{0}}\n\
             store_slice(dst, b.data[:])\n\
             task worker(move dst, done)\n\
             b.data[0] = 2\n\
             <-done",
    );
}

#[test]
fn closure_store_of_array_slice_freezes_the_array() {
    assert_e3003(
        "    done := make(chan bool, 1)\n\
             arr := [4]int{1, 2, 3, 4}\n\
             dst := [][]int{[]int{0}}\n\
             put := fn(d [][]int, s []int) {\n\
                 d[0] = s\n\
             }\n\
             put(dst, arr[:])\n\
             task worker(move dst, done)\n\
             arr[0] = 2\n\
             <-done",
    );
}

#[test]
fn copy_of_by_value_array_clone_slice_stays_accepted() {
    assert_accept(
        "    done := make(chan bool, 1)\n\
             arr := [4]int{1, 2, 3, 4}\n\
             dst := [][]int{[]int{0}}\n\
             copy(dst, [][]int{clone4(arr)[:]})\n\
             task worker(move dst, done)\n\
             arr[0] = 2\n\
             <-done",
    );
}

#[test]
fn elementwise_copy_of_primitive_array_slice_stays_accepted() {
    assert_accept(
        "    done := make(chan bool, 1)\n\
             arr := [4]int{1, 2, 3, 4}\n\
             ints := make([]int, 4)\n\
             copy(ints, arr[:])\n\
             task worker_ints(move ints, done)\n\
             arr[0] = 2\n\
             <-done",
    );
}

#[test]
fn helper_that_never_stores_its_argument_stays_accepted() {
    assert_accept(
        "    done := make(chan bool, 1)\n\
             arr := [4]int{1, 2, 3, 4}\n\
             dst := [][]int{[]int{0}}\n\
             peek_slice(dst, arr[:])\n\
             task worker(move dst, done)\n\
             arr[0] = 2\n\
             <-done",
    );
}

#[test]
fn copy_and_store_without_concurrency_boundary_stay_accepted() {
    assert_accept(
        "    arr := [4]int{1, 2, 3, 4}\n\
             dst := [][]int{[]int{0}}\n\
             copy(dst, [][]int{arr[:]})\n\
             store_slice(dst, arr[:])\n\
             arr[0] = 2\n\
             dst[0][0] = 3",
    );
}
