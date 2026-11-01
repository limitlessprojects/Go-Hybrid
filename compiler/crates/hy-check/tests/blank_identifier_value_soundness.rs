//! blank_identifier_value_soundness.rs — regression tests for Issue #2710:
//! verification that blank identifier `_` used as a value expression is rejected
//! with HYB-E1002 under Row 13 BUILDABLE.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn blank_identifier_in_assignment_value_rejected() {
    // 1. _ = _
    let src1 = "fn main() { _ = _ }";
    let r1 = check(src1);
    assert_eq!(r1.verdict, Verdict::Reject);
    assert!(error_codes(&r1).contains(&"HYB-E1002".to_string()));

    // 2. x := _
    let src2 = "fn main() {\n    x := _\n    _ = x\n}";
    let r2 = check(src2);
    assert_eq!(r2.verdict, Verdict::Reject);
    assert!(error_codes(&r2).contains(&"HYB-E1002".to_string()));
}

#[test]
fn blank_identifier_in_call_and_binary_expr_rejected() {
    // 1. Function argument: foo(_)
    let src1 = r#"
fn foo(x int) {}
fn main() {
    foo(_)
}
"#;
    let r1 = check(src1);
    assert_eq!(r1.verdict, Verdict::Reject);
    assert!(error_codes(&r1).contains(&"HYB-E1002".to_string()));

    // 2. Binary operation: _ + 1
    let src2 = r#"
fn main() {
    y := _ + 1
    _ = y
}
"#;
    let r2 = check(src2);
    assert_eq!(r2.verdict, Verdict::Reject);
    assert!(error_codes(&r2).contains(&"HYB-E1002".to_string()));
}

#[test]
fn blank_identifier_in_composite_literals_rejected() {
    // 1. Slice literal: []int{_}
    let src_slice = r#"
fn main() {
    s := []int{_}
    _ = s
}
"#;
    let r_slice = check(src_slice);
    assert_eq!(r_slice.verdict, Verdict::Reject);
    assert!(error_codes(&r_slice).contains(&"HYB-E1002".to_string()));

    // 2. Map literal: map[string]int{"a": _}
    let src_map = r#"
fn main() {
    m := map[string]int{"a": _}
    _ = m
}
"#;
    let r_map = check(src_map);
    assert_eq!(r_map.verdict, Verdict::Reject);
    assert!(error_codes(&r_map).contains(&"HYB-E1002".to_string()));

    // 3. Struct literal: point{x: _}
    let src_struct = r#"
struct point {
    x int
}
fn main() {
    p := point{x: _}
    _ = p
}
"#;
    let r_struct = check(src_struct);
    assert_eq!(r_struct.verdict, Verdict::Reject);
    assert!(error_codes(&r_struct).contains(&"HYB-E1002".to_string()));
}

#[test]
fn blank_identifier_in_return_and_channel_send_rejected() {
    // 1. Return statement: return _
    let src_ret = r#"
fn get_num() int {
    return _
}
fn main() {}
"#;
    let r_ret = check(src_ret);
    assert_eq!(r_ret.verdict, Verdict::Reject);
    assert!(error_codes(&r_ret).contains(&"HYB-E1002".to_string()));

    // 2. Channel send: ch <- _
    let src_send = r#"
fn main() {
    ch := make(chan int, 1)
    ch <- _
    _ = ch
}
"#;
    let r_send = check(src_send);
    assert_eq!(r_send.verdict, Verdict::Reject);
    assert!(error_codes(&r_send).contains(&"HYB-E1002".to_string()));
}

#[test]
fn valid_blank_identifier_in_place_positions_accepted() {
    // 1. Discarding assignment value: _ = 5
    let src_discard = "fn main() { _ = 5 }";
    let r_discard = check(src_discard);
    assert_eq!(r_discard.verdict, Verdict::Accept);

    // 2. Multi-value discard in map lookup
    let src_map = r#"
fn main() {
    m := map[string]int{"a": 1}
    _, ok := m["a"]
    _ = ok
}
"#;
    let r_map = check(src_map);
    assert_eq!(r_map.verdict, Verdict::Accept);
}
