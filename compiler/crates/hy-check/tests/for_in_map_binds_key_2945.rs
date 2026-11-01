//! Regression tests for #2945: SPEC LX13 says a single-variable
//! `for k in m` over a map ranges the KEYS. The checker bound the map's
//! VALUE kind, so a correct program was rejected and a wrong one passed
//! (and printed values instead of keys).

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let f = parse_file("test.hy", src).expect("parse");
    check_file(&f).diagnostics.into_iter().map(|d| d.code).collect()
}

fn main_body(body: &str) -> String {
    format!("fn main() {{\n{}\n}}\n", body)
}

fn rejects(body: &str) {
    let src = main_body(body);
    let c = codes(&src);
    assert!(
        c.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for:\n{}\ngot {:?}",
        src,
        c
    );
}

fn accepts(body: &str) {
    let src = main_body(body);
    let c = codes(&src);
    assert!(c.is_empty(), "expected 0 diagnostics for:\n{}\ngot {:?}", src, c);
}

#[test]
fn single_var_binds_key_kind() {
    accepts("m := map[string]int{\"a\": 1}\nfor k in m {\n    var s string = k\n    fmt.println(s)\n}");
}

#[test]
fn single_var_value_kind_rejected() {
    rejects("m := map[string]int{\"a\": 1}\nfor k in m {\n    var s int = k\n    fmt.println(s)\n}");
}

#[test]
fn int_keyed_map_key_kind() {
    accepts("m := map[int]string{1: \"a\"}\nfor k in m {\n    var n int = k\n    fmt.println(n)\n}");
    rejects("m := map[int]string{1: \"a\"}\nfor k in m {\n    var s string = k\n    fmt.println(s)\n}");
}

#[test]
fn two_var_still_key_value() {
    accepts("m := map[string]int{\"a\": 1}\nfor k, v in m {\n    var s string = k\n    var n int = v\n    fmt.println(s, n)\n}");
    rejects("m := map[string]int{\"a\": 1}\nfor k, v in m {\n    var n int = k\n    fmt.println(n, v)\n}");
}

#[test]
fn slice_single_var_still_element() {
    accepts("xs := []string{\"a\"}\nfor x in xs {\n    var s string = x\n    fmt.println(s)\n}");
    rejects("xs := []string{\"a\"}\nfor x in xs {\n    var n int = x\n    fmt.println(n)\n}");
}
