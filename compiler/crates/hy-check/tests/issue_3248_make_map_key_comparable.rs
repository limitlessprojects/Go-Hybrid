//! #3248 — `make()` must validate map-key comparability in its type argument.
//! Go forbids forming `map[K]V` with an unhashable `K` anywhere ("invalid map
//! key type"), and hy-check enforces that with HYB-E1002 in every declared
//! position (var decls, aliases, struct fields, fn params, map literals).
//! The `make()` type argument is a type position that skips the expression
//! walker, so map types nested inside chan / slice / array wrappers or aliases
//! reached the emitter unchecked.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn error_codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    res.diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code)
        .collect()
}

fn assert_reject(src: &str) {
    let codes = error_codes(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for unhashable map key, got: {:?}",
        codes
    );
}

fn assert_accept(src: &str) {
    let ast = parse_file("test.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    assert_eq!(res.verdict, Verdict::Accept, "expected Accept, got: {:?}", res.diagnostics);
    let errs: Vec<_> = res.diagnostics.into_iter().filter(|d| d.severity == Severity::Error).collect();
    assert!(errs.is_empty(), "expected 0 errors, got: {:?}", errs);
}

#[test]
fn reject_make_chan_of_map_with_slice_key() {
    assert_reject("fn main() {\n    ch := make(chan map[[]int]int, 1)\n    _ = ch\n}\n");
}

#[test]
fn reject_make_slice_of_map_with_slice_key() {
    assert_reject("fn main() {\n    s := make([]map[[]int]int, 1)\n    _ = s\n}\n");
}

#[test]
fn reject_make_chan_of_map_with_string_slice_key() {
    assert_reject("fn main() {\n    ch := make(chan map[[]string]int, 1)\n    _ = ch\n}\n");
}

#[test]
fn reject_make_chan_of_map_with_fn_key() {
    assert_reject("fn main() {\n    ch := make(chan map[fn()]int, 1)\n    _ = ch\n}\n");
}

#[test]
fn reject_make_chan_of_array_of_map_with_fn_key() {
    assert_reject("fn main() {\n    ch := make(chan [2]map[fn()]int, 1)\n    _ = ch\n}\n");
}

#[test]
fn reject_make_slice_of_map_with_fn_key() {
    assert_reject("fn main() {\n    s := make([]map[fn()]int, 0)\n    _ = s\n}\n");
}

#[test]
fn reject_make_chan_type_alias_with_unhashable_map_key() {
    let src = r#"
type bad_chan = chan map[[]int]int

fn main() {
    ch := make(bad_chan, 1)
    _ = ch
}
"#;
    assert_reject(src);
}

#[test]
fn reject_make_slice_type_alias_with_unhashable_map_key() {
    let src = r#"
type bad_slice = []map[[]int]int

fn main() {
    s := make(bad_slice, 1)
    _ = s
}
"#;
    assert_reject(src);
}

#[test]
fn reject_make_chan_with_unhashable_key_type_alias() {
    let src = r#"
type unhashable_key = []int

fn main() {
    ch := make(chan map[unhashable_key]int, 1)
    _ = ch
}
"#;
    assert_reject(src);
}

#[test]
fn accept_valid_make_map_key_types() {
    let src = r#"
type good_chan = chan map[int]string

fn main() {
    c1 := make(chan map[string]int, 1)
    s1 := make([]map[int]int, 1)
    c2 := make(chan [2]map[string]int, 1)
    c3 := make(good_chan, 1)
    _ = c1
    _ = s1
    _ = c2
    _ = c3
}
"#;
    assert_accept(src);
}
