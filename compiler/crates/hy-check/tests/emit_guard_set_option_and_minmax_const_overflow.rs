use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let file = parse_file("test.hy", src).expect("parse ok");
    check_file(&file)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn test_set_insert_remove_contains_and_option_or_const_overflow_emit_hyb_e6002() {
    // Prelude `set[T]` methods (`insert`, `remove`, `contains`) and `option[T].or`
    // must check constant overflow of their argument against element type `T` (HYB-E6002).
    let src_set = r#"
fn bad_set() {
    var s set[int8] = set[int8]{}
    s.insert(300)
    s.remove(-200)
    _ = s.contains(128)
}
"#;
    assert_eq!(
        codes(src_set),
        vec!["HYB-E6002", "HYB-E6002", "HYB-E6002"]
    );

    let src_opt = r#"
fn bad_opt(x option[uint8]) uint8 {
    return x.or(300)
}
"#;
    assert_eq!(codes(src_opt), vec!["HYB-E6002"]);
}

#[test]
fn test_min_max_typed_and_default_const_overflow_emit_hyb_e6002() {
    // Built-in `min` and `max` unify untyped constant arguments with any typed
    // operand, so out-of-range constants must emit HYB-E6002 instead of failing `go build`.
    let src_minmax = r#"
fn bad_minmax(x int8, u uint8, n int) {
    _ = min(x, 300)
    _ = max(-1, u)
    _ = min(n, 1 << 63)
}
"#;
    assert_eq!(
        codes(src_minmax),
        vec!["HYB-E6002", "HYB-E6002", "HYB-E6002"]
    );
}

#[test]
fn test_positive_in_range_set_option_and_minmax_calls_accepted() {
    let src_ok = r#"
fn ok(i int8, x option[uint8], u uint8) uint8 {
    var s set[int8] = set[int8]{}
    s.insert(127)
    s.remove(-128)
    _ = s.contains(0)
    _ = min(i, 100)
    _ = max(0, u)
    _ = min(1, 2)
    return x.or(255)
}
"#;
    assert!(codes(src_ok).is_empty());
}
