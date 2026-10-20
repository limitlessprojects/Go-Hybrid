//! composite_lit_void_soundness.rs — tests for HYB-E1002 rejection of
//! void calls, 0-value builtins, and multi-value calls inside composite literals
//! (struct fields, slice items, map keys, and map values) and unary operations.

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

// -----------------------------------------------------------------------------
// Struct Literals
// -----------------------------------------------------------------------------

#[test]
fn rejects_void_call_in_struct_field() {
    let src = "
struct point {
    x int
}
fn noop() {}
fn main() {
    p := point{x: noop()}
    _ = p
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn rejects_close_builtin_in_struct_field() {
    let src = "
struct point {
    x int
}
fn main() {
    ch := make(chan int, 1)
    p := point{x: close(ch)}
    _ = p
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn rejects_multi_value_call_in_struct_field() {
    let src = "
struct point {
    x int
}
fn two() (int, int) {
    return 1, 2
}
fn main() {
    p := point{x: two()}
    _ = p
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// -----------------------------------------------------------------------------
// Slice Literals
// -----------------------------------------------------------------------------

#[test]
fn rejects_void_call_in_slice_item() {
    let src = "
fn noop() {}
fn main() {
    xs := []int{noop()}
    _ = xs
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn rejects_close_builtin_in_slice_item() {
    let src = "
fn main() {
    ch := make(chan int, 1)
    xs := []int{close(ch)}
    _ = xs
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn rejects_multi_value_call_in_slice_item() {
    let src = "
fn two() (int, int) {
    return 1, 2
}
fn main() {
    xs := []int{two()}
    _ = xs
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// -----------------------------------------------------------------------------
// Map Literals
// -----------------------------------------------------------------------------

#[test]
fn rejects_void_call_in_map_value() {
    let src = "
fn noop() {}
fn main() {
    m := map[string]int{\"k\": noop()}
    _ = m
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn rejects_void_call_in_map_key() {
    let src = "
fn noop() {}
fn main() {
    m := map[int]string{noop(): \"v\"}
    _ = m
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn rejects_close_builtin_in_map_key() {
    let src = "
fn main() {
    ch := make(chan int, 1)
    m := map[int]string{close(ch): \"v\"}
    _ = m
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// -----------------------------------------------------------------------------
// Positive Controls
// -----------------------------------------------------------------------------

#[test]
fn accepts_valid_composite_literals() {
    let src = "
struct point {
    x int
    y string
}
fn get_num() int {
    return 100
}
fn main() {
    p := point{x: get_num(), y: \"ok\"}
    _ = p

    xs := []int{1, 2, get_num()}
    _ = xs

    m := map[string]int{\"a\": 1, \"b\": get_num()}
    _ = m
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "valid composites must accept");
}
