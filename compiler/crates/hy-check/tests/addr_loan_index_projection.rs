use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn error_codes(src: &str) -> Vec<String> {
    let ast = parse_file("inline.hy", src).unwrap_or_else(|d| panic!("parse failed: {:?}", d));
    check_file(&ast)
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code)
        .collect()
}

#[test]
fn index_and_slice_projections_on_slot_loan_reject_e3104() {
    // `&n.arr[0]` on a struct map-slot loan `n := &m["a"]`.
    let field_idx_codes = error_codes(
        r#"struct node { arr [4]int }
fn main() {
    m := map[string]node{"a": node{arr: [4]int{1, 2, 3, 4}}}
    n := &m["a"]
    p := &n.arr[0]
    fmt.println(p)
}
"#,
    );
    assert!(
        field_idx_codes.contains(&"HYB-E3104".to_string()),
        "got {:?}",
        field_idx_codes
    );

    // Direct index projection `&n[0]` on an array-valued map-slot loan `n := &m["a"]`.
    let direct_idx_codes = error_codes(
        r#"fn main() {
    m := map[string][4]int{"a": [4]int{1, 2, 3, 4}}
    n := &m["a"]
    p := &n[0]
    fmt.println(p)
}
"#,
    );
    assert!(
        direct_idx_codes.contains(&"HYB-E3104".to_string()),
        "got {:?}",
        direct_idx_codes
    );

    // Call-argument and closure-body index projections on a map-slot loan.
    let call_arg_codes = error_codes(
        r#"struct node { arr [4]int }
fn touch(p *int) {}
fn main() {
    m := map[string]node{"a": node{arr: [4]int{1, 2, 3, 4}}}
    n := &m["a"]
    touch(&n.arr[0])
}
"#,
    );
    assert!(
        call_arg_codes.contains(&"HYB-E3104".to_string()),
        "got {:?}",
        call_arg_codes
    );

    let slice_proj_codes = error_codes(
        r#"struct node { arr [4]int }
fn main() {
    m := map[string]node{"a": node{arr: [4]int{1, 2, 3, 4}}}
    n := &m["a"]
    p := &n.arr[0:2]
    fmt.println(p)
}
"#,
    );
    assert!(
        slice_proj_codes.contains(&"HYB-E3104".to_string()),
        "got {:?}",
        slice_proj_codes
    );
}
