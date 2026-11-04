//! #3068 — `new(T)` result kind must resolve to `Pointer(T)`, not `Pointer(Unknown)`.
//!
//! Prior to this fix, `call_result_kinds` in `buildable.rs` returned
//! `Some(vec![OpKind::Pointer(Box::new(OpKind::Unknown))])` for `new`.
//! Because `kinds_conflict` is fail-open on `Unknown`, `new(int)` was accepted
//! where `*string`, `*point`, or any other pointer type was expected.
//! The emitted Go code then failed `go build` with mismatched type errors,
//! violating GUARANTEES.md Row 13 (BUILDABLE).
//!
//! This suite verifies:
//!   - Mismatched pointer returns (`return new(int)` from `fn() *string`) are rejected (HYB-E1002).
//!   - Mismatched struct pointer returns (`return new(node)` from `fn() *point`) are rejected (HYB-E1002).
//!   - Mismatched pointer variable initializers (`var p *string = new(int)`) are rejected (HYB-E1002).
//!   - Mismatched pointer assignments (`p = new(int)`) are rejected (HYB-E1002).
//!   - Mismatched pointer call arguments (`take_str(new(int))`) are rejected (HYB-E1002).
//!   - Mismatched pointer comparisons (`new(int) == new(string)`) are rejected (HYB-E1002).
//!   - Mismatched slice pointer returns (`return new([]int)` from `fn() *[]string`) are rejected (HYB-E1002).
//!   - Mismatched map-alias pointer returns (`return new(int_map)` from `fn() *str_map`) are rejected (HYB-E1002).
//!   - Mismatched scalar-alias pointer returns (`return new(user_id)` from `fn() *string`) are rejected (HYB-E1002).
//!   - Valid matching pointer operations and nil comparisons stay accepted (0 diagnostics).

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("parses");
    check_file(&ast)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn assert_contains_code(src: &str, expected: &str) {
    let cs = codes(src);
    assert!(
        cs.iter().any(|c| c == expected),
        "expected diagnostic {expected}, got {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(cs.is_empty(), "expected 0 diagnostics, got {cs:?} for:\n{src}");
}

#[test]
fn test_new_return_mismatched_scalar_pointer_rejected() {
    let src = r#"
package test

fn bad_ret() *string {
    return new(int)
}
"#;
    assert_contains_code(src, "HYB-E1002");
}

#[test]
fn test_new_return_mismatched_struct_pointer_rejected() {
    let src = r#"
package test

struct point {
    x int
}

struct node {
    val string
}

fn bad_ret() *point {
    return new(node)
}
"#;
    assert_contains_code(src, "HYB-E1002");
}

#[test]
fn test_new_var_init_mismatched_pointer_rejected() {
    let src = r#"
package test

fn bad_init() {
    var p *string = new(int)
    _ = p
}
"#;
    assert_contains_code(src, "HYB-E1002");
}

#[test]
fn test_new_assign_mismatched_pointer_rejected() {
    let src = r#"
package test

fn bad_assign() {
    var p *string
    p = new(int)
    _ = p
}
"#;
    assert_contains_code(src, "HYB-E1002");
}

#[test]
fn test_new_call_arg_mismatched_pointer_rejected() {
    let src = r#"
package test

fn take_str(p *string) {
    _ = p
}

fn bad_call() {
    take_str(new(int))
}
"#;
    assert_contains_code(src, "HYB-E1002");
}

#[test]
fn test_new_equality_mismatched_pointer_rejected() {
    let src = r#"
package test

fn bad_eq() bool {
    return new(int) == new(string)
}
"#;
    assert_contains_code(src, "HYB-E1002");
}

#[test]
fn test_new_slice_mismatched_pointer_rejected() {
    let src = r#"
package test

fn bad_slice_ptr() *[]string {
    return new([]int)
}
"#;
    assert_contains_code(src, "HYB-E1002");
}

#[test]
fn test_new_map_alias_mismatched_pointer_rejected() {
    let src = r#"
package test

type int_map = map[string]int
type str_map = map[string]string

fn bad_map_ptr() *str_map {
    return new(int_map)
}
"#;
    assert_contains_code(src, "HYB-E1002");
}

#[test]
fn test_new_type_alias_mismatched_pointer_rejected() {
    let src = r#"
package test

type user_id = int

fn bad_alias_ptr() *string {
    return new(user_id)
}
"#;
    assert_contains_code(src, "HYB-E1002");
}

#[test]
fn test_positive_controls_matching_pointers() {
    let src = r#"
package test

struct point {
    x int
}

type user_id = int
type int_map = map[string]int

fn take_int_ptr(p *int) {
    _ = p
}

fn ok_ret_int() *int {
    return new(int)
}

fn ok_ret_str() *string {
    return new(string)
}

fn ok_ret_struct() *point {
    return new(point)
}

fn ok_ret_slice() *[]int {
    return new([]int)
}

fn ok_ret_map() *int_map {
    return new(int_map)
}

fn ok_ret_alias() *user_id {
    return new(user_id)
}

fn ok_ops() {
    var p *int = new(int)
    p = new(int)
    take_int_ptr(new(int))

    var p1 *int = new(int)
    var p2 *int = new(int)
    _ = p1 == p2
    _ = p1 == nil
    _ = nil == p2
    _ = p
}
"#;
    assert_ok(src);
}
