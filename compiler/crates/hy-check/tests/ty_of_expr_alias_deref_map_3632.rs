use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn ty_of_expr_and_multi_value_tys_resolve_alias_and_pointer_for_e6002_3632() {
    // 1. Exact issue #3632 reproducer rejects fail-closed with HYB-E6002
    let repro = r#"
type byte_slice = []u8
type byte_map = map[string]u8

fn main() {
    xs := make(byte_slice, 1)
    xs[0] = 300

    m := make(byte_map)
    m["a"] = 300

    v, ok := m["a"]
    v = 300
    _ = ok
}
"#;
    let r = check_file(&parse_file("repro.hy", repro).unwrap());
    assert_eq!(r.verdict, Verdict::Reject, "reproducer must reject");
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E6002"),
        "expected HYB-E6002 diagnostic in: {:?}",
        r.diagnostics
    );

    // 2. Pointer dereferenced indexing in ty_of_expr
    let ptr_src = r#"
fn check_ptr(p: *[2]u8) {
    x := p[0]
    x = 300
}
"#;
    let r = check_file(&parse_file("ptr.hy", ptr_src).unwrap());
    assert_eq!(r.verdict, Verdict::Reject, "pointer indexing must reject");
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E6002"),
        "expected HYB-E6002 on pointer indexing: {:?}",
        r.diagnostics
    );

    // 3. Valid in-range assignments accept cleanly (negative control)
    let valid_src = r#"
type byte_slice = []u8
type byte_map = map[string]u8

fn main() {
    xs := make(byte_slice, 1)
    xs[0] = 42

    m := make(byte_map)
    m["a"] = 42

    v, ok := m["a"]
    _ = v
    _ = ok
}
"#;
    let r = check_file(&parse_file("valid.hy", valid_src).unwrap());
    assert_eq!(r.verdict, Verdict::Accept, "valid values must accept: {:?}", r.diagnostics);
}
