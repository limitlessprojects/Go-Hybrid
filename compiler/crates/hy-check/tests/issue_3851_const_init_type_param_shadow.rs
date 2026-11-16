
use hy_check::check_file;
use hy_syntax::parse_file;

fn get_codes(src: &str) -> Vec<String> {
    let f = parse_file("test.hy", src).expect("parse ok");
    check_file(&f).diagnostics.into_iter().map(|d| d.code).collect()
}

#[test]
fn test_reproducer_3851() {
    let src = r#"
type my_int = int
const k = 42
fn f[my_int, k]() {
    const c = my_int(1)
    const d my_int = 1
    const e = k + 1
    _ = c
    _ = d
    _ = e
}
fn main() {}
"#;
    assert_eq!(get_codes(src), vec!["HYB-E6003", "HYB-E6003", "HYB-E6003"]);
}

#[test]
fn test_string_len_type_param_shadow() {
    let src_shadowed = r#"
const s = "abc"
fn f[s]() {
    const n = len(s)
    _ = n
}
fn main() {}
"#;
    let codes = get_codes(src_shadowed);
    assert!(codes.contains(&"HYB-E6003".to_string()));

    let src_unshadowed = r#"
type my_int = int
const k = 42
const s = "abc"
fn g() {
    const c = my_int(1)
    const d my_int = 1
    const e = k + 1
    const n = len(s)
    _ = c
    _ = d
    _ = e
    _ = n
}
fn main() {}
"#;
    assert!(get_codes(src_unshadowed).is_empty());
}
