//! #3248 — `make()` must validate map-key comparability in its type argument.
//! Go forbids forming `map[K]V` with an unhashable `K` anywhere ("invalid map
//! key type"), and hy-check enforces that with HYB-E1002 in every declared
//! position (var decls, aliases, struct fields, fn params, map literals).
//! The `make()` type argument is a type position that skips the expression
//! walker, so map types nested inside chan / slice / array wrappers reached
//! the emitter unchecked.

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

fn assert_reject(src: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

fn assert_accept(src: &str) {
    let r = check(src);
    assert!(error_codes(&r).is_empty(), "{:?}", r.diagnostics);
}

fn make_prog(make_expr: &str) -> String {
    format!("fn main() {{\n    v := {make_expr}\n    _ = v\n}}\n")
}

#[test]
fn reject_make_chan_of_map_with_slice_key() {
    assert_reject(&make_prog("make(chan map[[]int]int, 1)"));
}

#[test]
fn reject_make_slice_of_map_with_slice_key() {
    assert_reject(&make_prog("make([]map[[]int]int, 1)"));
}

#[test]
fn reject_make_chan_of_map_with_fn_key() {
    assert_reject(&make_prog("make(chan map[fn()]int, 1)"));
}

#[test]
fn reject_make_chan_of_array_of_map_with_fn_key() {
    assert_reject(&make_prog("make(chan [2]map[fn()]int, 1)"));
}

#[test]
fn reject_make_slice_of_map_with_fn_key() {
    assert_reject(&make_prog("make([]map[fn()]int, 0)"));
}

#[test]
fn accept_make_chan_of_map_with_string_key() {
    assert_accept(&make_prog("make(chan map[string]int, 1)"));
}

#[test]
fn accept_make_slice_of_map_with_int_key() {
    assert_accept(&make_prog("make([]map[int]string, 0)"));
}

#[test]
fn accept_make_chan_of_int() {
    assert_accept(&make_prog("make(chan int, 1)"));
}

#[test]
fn accept_make_slice_of_int() {
    assert_accept(&make_prog("make([]int, 3)"));
}

#[test]
fn accept_make_chan_of_map_with_slice_value() {
    // A slice as the map VALUE is legal; only the key must be comparable.
    assert_accept(&make_prog("make(chan map[string][]int, 1)"));
}

#[test]
fn accept_make_chan_of_array_of_map_with_string_key() {
    assert_accept(&make_prog("make(chan [2]map[string]int, 1)"));
}
