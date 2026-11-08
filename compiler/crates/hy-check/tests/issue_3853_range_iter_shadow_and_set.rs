
use hy_check::check_file;
use hy_syntax::parse_file;

fn get_codes(src: &str) -> Vec<String> {
    let f = parse_file("test.hy", src).expect("parse ok");
    check_file(&f).diagnostics.into_iter().map(|d| d.code).collect()
}

#[test]
fn test_reproducer_3853() {
    let src = r#"
const a = 1.5
const b = a
enum set { x }
fn f(s set) {
    a := 1
    _ = a
    for i in 0..b { _ = i }
    for v in s { _ = v }
}
fn g[set](s set) {
    for v in s { _ = v }
}
fn main() {}
"#;
    let codes = get_codes(src);
    assert_eq!(codes, vec!["HYB-E2009", "HYB-E2009", "HYB-E3005"]);
}

#[test]
fn test_chained_typed_const_bounds_and_alias_conversions() {
    let src_mismatch_chained = r#"
const a u32 = 1
const b = a
fn f() {
    var x u64 = 10
    for i in b..x { _ = i }
}
fn main() {}
"#;
    assert_eq!(get_codes(src_mismatch_chained), vec!["HYB-E2009"]);

    let src_mismatch_call = r#"
const b = u32(1)
fn f() {
    var x u64 = 10
    for i in b..x { _ = i }
}
fn main() {}
"#;
    assert_eq!(get_codes(src_mismatch_call), vec!["HYB-E2009"]);

    let src_mismatch_alias_call = r#"
type my_u32 = u32
fn f() {
    var x u64 = 10
    for i in my_u32(0)..x { _ = i }
}
fn main() {}
"#;
    assert_eq!(get_codes(src_mismatch_alias_call), vec!["HYB-E2009"]);

    let src_valid = r#"
const a u32 = 1
const b = a
type my_u32 = u32
fn f() {
    var x u32 = 10
    for i in b..x { _ = i }
    for i in my_u32(0)..x { _ = i }
}
fn main() {}
"#;
    assert!(get_codes(src_valid).is_empty());
}
