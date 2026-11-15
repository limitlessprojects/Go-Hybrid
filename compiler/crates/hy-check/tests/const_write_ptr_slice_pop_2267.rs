//! Regression tests for #2267: T22 (`HYB-E3006`) const-write checks on pointer/non-ident slice `.pop()`.

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let parsed = parse_file("test.hy", src).expect("parse should succeed");
    let out = check_file(&parsed);
    out.diagnostics.into_iter().map(|d| d.code).collect()
}

#[test]
fn rejects_const_slice_pop_via_pointer_and_non_ident_root() {
    let src = r#"
package main

fn id_slice(s []int) []int {
    return s
}

fn main() {
    const xs = []int{10, 20, 30}
    let _a = (&xs).pop()
    let p = &xs
    let _b = p.pop()
    let _c = id_slice(xs).pop()
}
"#;
    let c = codes(src);
    let e3006_count = c.iter().filter(|x| x.as_str() == "HYB-E3006").count();
    assert!(
        e3006_count >= 3,
        "expected >=3 HYB-E3006 diagnostics for (&xs).pop(), p.pop(), and id_slice(xs).pop(), got: {:?}",
        c
    );
}
