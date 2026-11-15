//! Composite literal elements/keys/values whose composite INNER type differs
//! (#3412).
//!
//! The kind model collapses container inners — `OpKind::Chan(Int)` fits
//! `chan int64` as well as `chan int32` — so `[]chan int32{ bad_chan }`,
//! `[1][3]int{ bad_arr }` and `map[string]chan int32{ "k": bad_chan }` all
//! passed `hyb check` with 0 diagnostics while failing `go build` (`cannot
//! use ... as ... value in slice/array/map literal`). `deep_composite_conflict`
//! compares the resolved spellings part by part; channel direction
//! mismatches stay with the E3012 coercion layer, and aliases/type
//! parameters fail open.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_has(r: &hy_check::CheckResult, code: &str) {
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == code),
        "expected {}, got {:?}",
        code,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

fn assert_clean(r: &hy_check::CheckResult) {
    let cs = error_codes(&r);
    assert!(cs.is_empty(), "expected clean accept, got {:?}", cs);
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn slice_lit_inner_chan_mismatch_rejects_e1002() {
    assert_has(
        &check(
            r#"package main

fn sink[t any](x t) {}

fn main() {
    var bad_chan chan int64
    let s = []chan int32{ bad_chan }
    sink(s)
}
"#,
        ),
        "HYB-E1002",
    );
}

#[test]
fn array_lit_inner_len_mismatch_rejects_e1002() {
    assert_has(
        &check(
            r#"package main

fn sink[t any](x t) {}

fn main() {
    var bad_arr [4]int
    let a = [1][3]int{ bad_arr }
    sink(a)
}
"#,
        ),
        "HYB-E1002",
    );
}

#[test]
fn map_lit_value_inner_chan_mismatch_rejects_e1002() {
    assert_has(
        &check(
            r#"package main

fn sink[t any](x t) {}

fn main() {
    var bad_chan chan int64
    let m = map[string]chan int32{ "k": bad_chan }
    sink(m)
}
"#,
        ),
        "HYB-E1002",
    );
}

#[test]
fn matching_inners_and_chan_direction_assignability_accept() {
    // Identical inners, a bidirectional source into a receive-only element
    // (spec §Assignability), pointers, arrays and map values.
    assert_clean(&check(
        r#"package main

struct box {
    val int
}

fn sink[t any](x t) {}

fn main() {
    var ok_chan chan int32
    let s = []chan int32{ ok_chan }
    sink(s)

    var bidir chan int32
    let r = []<-chan int32{ bidir }
    sink(r)

    var ok_arr [3]int
    let a = [1][3]int{ ok_arr }
    sink(a)

    var ok_ptr *box
    let p = []*box{ ok_ptr }
    sink(p)

    let m = map[string]chan int32{ "k": ok_chan }
    sink(m)
}
"#,
    ));
}

#[test]
fn direction_mismatch_in_slice_lit_stays_with_e3012_not_double_reported() {
    // `chan<- int` into a `<-chan int` element: the E3012 coercion layer
    // owns direction; no second E1002 on the same span.
    let r = check(
        r#"package main

fn sink[t any](x t) {}

fn main() {
    var send_only chan<- int32
    let r = []<-chan int32{ send_only }
    sink(r)
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3012"),
        "expected HYB-E3012, got {:?}",
        cs
    );
    assert!(
        !cs.iter().any(|c| c == "HYB-E1002"),
        "direction mismatch must not double-report E1002, got {:?}",
        cs
    );
}
