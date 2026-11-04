use hy_check::check_file;
use hy_syntax::parse_file;

fn has_code(src: &str, code: &str) -> bool {
    let file = parse_file("test.hy", src).expect("parse should succeed");
    check_file(&file).diagnostics.iter().any(|d| d.code == code)
}

#[test]
fn generic_fn_body_equality_conversion_and_map_key_729() {
    assert!(has_code("fn eq_any[t any](a t, b t) bool { return a == b }\nfn main() {}\n", "HYB-E2006"));
    assert!(has_code("fn diff[t comparable, u comparable](a t, b u) bool { return a == b }\nfn main() {}\n", "HYB-E1002"));
    assert!(has_code("fn bad_conv[t any](x int) t { return t(x) }\nfn main() {}\n", "HYB-E1002"));
    assert!(has_code("fn bad_map[k any, v any]() map[k]v { return map[k]v{} }\nfn main() {}\n", "HYB-E1002"));
    let f = parse_file("test.hy", "fn make_map[k, v](key k, val v) map[k]v {\n    m := map[k]v{}\n    m[key] = val\n    return m\n}\nfn main() { _ = make_map(\"a\", 1) }\n").unwrap();
    assert!(check_file(&f).diagnostics.is_empty());
}
