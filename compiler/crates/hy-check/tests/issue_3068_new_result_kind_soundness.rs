//! Regression tests for Issue #3068:
//! [soundness] hy-check: new(T) result kind is Pointer(Unknown), letting mismatched pointer returns and assignments pass check (Row 13 BUILDABLE).
//!
//! Under GUARANTEES.md Row 13 (BUILDABLE), any program accepted by `hyb check`
//! must emit Go code that passes `go build`.

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

fn assert_e1002(src: &str) {
    let cs = codes(src);
    assert!(
        !cs.is_empty() && cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(
        cs.is_empty(),
        "expected 0 diagnostics, got {cs:?} for:\n{src}"
    );
}

#[test]
fn test_rejects_mismatched_pointer_return_int_to_str() {
    assert_e1002(
        r#"
fn f() *string {
    return new(int)
}
fn main() {}
"#,
    );
}

#[test]
fn test_rejects_mismatched_pointer_return_str_to_int() {
    assert_e1002(
        r#"
fn f() *int {
    return new(string)
}
fn main() {}
"#,
    );
}

#[test]
fn test_rejects_mismatched_pointer_var_decl() {
    assert_e1002(
        r#"
fn main() {
    var p *string = new(int)
    _ = p
}
"#,
    );
}

#[test]
fn test_rejects_mismatched_pointer_assignment() {
    // Note: uninitialized var p *string produces HYB-E1002 (uninitialized variable).
    // Initialize p to *string and then assign mismatched new(int).
    assert_e1002(
        r#"
fn main() {
    var p *string = new(string)
    p = new(int)
    _ = p
}
"#,
    );
}

#[test]
fn test_rejects_mismatched_pointer_call_argument() {
    assert_e1002(
        r#"
fn take_str_ptr(p *string) {}
fn main() {
    take_str_ptr(new(int))
}
"#,
    );
}

#[test]
fn test_rejects_mismatched_struct_pointer_return() {
    assert_e1002(
        r#"
struct alpha {
    x int
}
struct beta {
    x int
}
fn f() *alpha {
    return new(beta)
}
fn main() {}
"#,
    );
}

#[test]
fn test_rejects_mismatched_slice_pointer_return() {
    assert_e1002(
        r#"
fn f() *[]string {
    return new([]int)
}
fn main() {}
"#,
    );
}

#[test]
fn test_rejects_mismatched_map_alias_pointer_return() {
    assert_e1002(
        r#"
type int_map = map[string]int
type str_map = map[string]string
fn f() *str_map {
    return new(int_map)
}
fn main() {}
"#,
    );
}

#[test]
fn test_accepts_matching_primitive_pointers() {
    assert_ok(
        r#"
fn f_int() *int {
    return new(int)
}
fn f_str() *string {
    return new(string)
}
fn take_int_ptr(p *int) {}
fn main() {
    var p1 *int = new(int)
    var p2 *string = new(string)
    p1 = new(int)
    take_int_ptr(new(int))
    _ = p1
    _ = p2
}
"#,
    );
}

#[test]
fn test_accepts_matching_struct_pointers() {
    assert_ok(
        r#"
struct point {
    x int
    y int
}
fn f() *point {
    return new(point)
}
fn take_point(p *point) {}
fn main() {
    var pt *point = new(point)
    pt = new(point)
    take_point(new(point))
    _ = pt
}
"#,
    );
}

#[test]
fn test_accepts_matching_slice_and_map_pointers() {
    assert_ok(
        r#"
type int_map = map[string]int
fn f_slice() *[]int {
    return new([]int)
}
fn f_map() *int_map {
    return new(int_map)
}
fn main() {
    var ps *[]int = new([]int)
    var pm *int_map = new(int_map)
    _ = ps
    _ = pm
}
"#,
    );
}
