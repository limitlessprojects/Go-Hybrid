//! issue_2875_composite_types_element_width_min_max.rs — regression tests for #2875
//!
//! Reject composite types differing only in element integer/float width
//! ([]i64→[]int, [2]i64→[2]int, map[string]i64→map[string]int, *i64→*int)
//! in var declarations, plain assignments, call arguments, and return statements.
//! Also reject min/max called with mixed numeric widths of typed variables.
//! GUARANTEES.md Row 13 (BUILDABLE) / HYB-E1002.

use hy_check::check_file;
use hy_syntax::parse_file;

fn error_codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("must parse");
    let res = check_file(&ast);
    res.diagnostics.into_iter().map(|d| d.code).collect()
}

#[test]
fn test_slice_element_width_var_decl_rejected() {
    let src = r#"
fn main() {
    var a []i64
    var b []int = a
    _ = b
}
"#;
    let errs = error_codes(src);
    assert!(
        errs.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for []i64 -> []int in var decl, got {:?}",
        errs
    );
}

#[test]
fn test_map_value_width_var_decl_rejected() {
    let src = r#"
fn main() {
    var a map[string]i64
    var b map[string]int = a
    _ = b
}
"#;
    let errs = error_codes(src);
    assert!(
        errs.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for map[string]i64 -> map[string]int in var decl, got {:?}",
        errs
    );
}

#[test]
fn test_pointer_base_width_var_decl_rejected() {
    let src = r#"
fn main() {
    var v i64 = 1
    p := &v
    var q *int = p
    _ = q
}
"#;
    let errs = error_codes(src);
    assert!(
        errs.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for *i64 -> *int in var decl, got {:?}",
        errs
    );
}

#[test]
fn test_array_element_width_var_decl_rejected() {
    let src = r#"
fn main() {
    var a [2]i64
    var b [2]int = a
    _ = b
}
"#;
    let errs = error_codes(src);
    assert!(
        errs.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for [2]i64 -> [2]int in var decl, got {:?}",
        errs
    );
}

#[test]
fn test_call_argument_composite_width_rejected() {
    let src = r#"
fn take(xs []int) int {
    return len(xs)
}
fn main() {
    var a []i64
    _ = take(a)
}
"#;
    let errs = error_codes(src);
    assert!(
        errs.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for []i64 argument to []int parameter, got {:?}",
        errs
    );
}

#[test]
fn test_return_composite_width_rejected() {
    let src = r#"
fn mk() []int {
    var a []i64
    return a
}
fn main() {
    _ = mk()
}
"#;
    let errs = error_codes(src);
    assert!(
        errs.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for return []i64 from []int function, got {:?}",
        errs
    );
}

#[test]
fn test_assignment_composite_width_rejected() {
    let src = r#"
fn main() {
    var a []i64
    var c []int
    c = a
    _ = c
}
"#;
    let errs = error_codes(src);
    assert!(
        errs.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for assigning []i64 to []int target, got {:?}",
        errs
    );
}

#[test]
fn test_min_max_mismatched_integer_widths_rejected() {
    let src = r#"
fn main() {
    var a i64 = 1
    var b int = 2
    _ = min(a, b)
    _ = max(b, a)
}
"#;
    let errs = error_codes(src);
    assert!(
        errs.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for min(i64, int), got {:?}",
        errs
    );
}

#[test]
fn test_min_max_mismatched_float_widths_rejected() {
    let src = r#"
fn main() {
    var a f32 = 1.0
    var b f64 = 2.0
    _ = max(a, b)
}
"#;
    let errs = error_codes(src);
    assert!(
        errs.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for max(f32, f64), got {:?}",
        errs
    );
}

#[test]
fn test_min_mixed_constant_and_mismatched_scalars_rejected() {
    let src = r#"
fn main() {
    var a i64 = 1
    var b int = 2
    _ = min(10, a, b)
}
"#;
    let errs = error_codes(src);
    assert!(
        errs.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for min(10, i64, int), got {:?}",
        errs
    );
}

#[test]
fn test_identical_composite_and_min_max_controls_accepted() {
    let src = r#"
fn take(xs []i64) int {
    return len(xs)
}

fn mk() []i64 {
    var a []i64
    return a
}

fn main() {
    var a []i64
    var b []i64 = a
    _ = b

    var m1 map[string]i64
    var m2 map[string]i64 = m1
    _ = m2

    var arr1 [2]i64
    var arr2 [2]i64 = arr1
    _ = arr2

    var v i64 = 1
    p := &v
    var q *i64 = p
    _ = q

    var ch1 chan i64
    var ch2 chan i64 = ch1
    _ = ch2

    _ = take(a)
    _ = mk()

    var c []i64
    c = a
    _ = c

    var x i64 = 1
    var y i64 = 2
    _ = min(x, y)

    var f1 f64 = 1.0
    var f2 f64 = 2.0
    _ = max(f1, f2)

    _ = min(1, 2)
}
"#;
    let errs = error_codes(src);
    assert!(
        errs.is_empty(),
        "expected 0 diagnostics for identical composite controls, got {:?}",
        errs
    );
}
