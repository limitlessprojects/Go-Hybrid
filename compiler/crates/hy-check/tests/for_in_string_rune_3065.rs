//! Regression tests for #3065: SPEC LX13 says `for x in s` over a
//! string ranges RUNES. The checker bound the element as an untyped int,
//! so byte/int uses passed hyb check and failed go build (rune vs byte).

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

const S: &str = "s := \"ab\"\n";

#[test]
fn rune_elem_into_byte_rejected() {
    rejects(&format!("{S}for c in s {{\n    var b byte = c\n    fmt.println(b)\n}}"));
}

#[test]
fn rune_elem_into_int_rejected() {
    rejects(&format!("{S}for c in s {{\n    var n int = c\n    fmt.println(n)\n}}"));
}

#[test]
fn rune_elem_mixed_with_int_rejected() {
    rejects(&format!("{S}n := 0\nfor c in s {{\n    n = n + c\n}}\nfmt.println(n)"));
}

#[test]
fn rune_elem_compared_with_byte_rejected() {
    rejects(&format!("{S}var b byte = 1\nfor c in s {{\n    if c == b {{ fmt.println(\"eq\") }}\n}}"));
}

#[test]
fn rune_elem_assigned_to_byte_rejected() {
    rejects(&format!("{S}var b byte = 'a'\nfor c in s {{\n    b = c\n}}\nfmt.println(b)"));
}

#[test]
fn two_var_rune_elem_into_byte_rejected() {
    rejects(&format!("{S}for i, c in s {{\n    var b byte = c\n    fmt.println(i, b)\n}}"));
}

#[test]
fn rune_elem_as_rune_and_i32_accepted() {
    accepts(&format!("{S}for c in s {{\n    var r rune = c\n    var i i32 = c\n    fmt.println(r, i)\n}}"));
}

#[test]
fn two_var_index_is_int_accepted() {
    accepts(&format!("{S}for i, c in s {{\n    var n int = i\n    var r rune = c\n    fmt.println(n, r)\n}}"));
}

#[test]
fn rune_elem_print_accepted() {
    accepts(&format!("{S}for c in s {{\n    fmt.println(c)\n}}"));
}

#[test]
fn byte_slice_elem_unchanged() {
    accepts(&format!("bs := []byte{{1, 2}}\nfor b in bs {{\n    var x byte = b\n    fmt.println(x)\n}}"));
}
