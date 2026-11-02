//! Regression test for Issue #2500:
//! Composite literal item, key, value, and struct field single-value context soundness (HYB-E1002).
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
fn slice_literal_void_and_multivalue_rejected() {
    let void_src = r#"
fn void_fn() {}

fn main() {
    xs := []int{void_fn()}
    _ = xs
}
"#;
    assert_e1002(void_src);

    let multi_src = r#"
fn two_vals() (int, int) {
    return 1, 2
}

fn main() {
    xs := []int{two_vals()}
    _ = xs
}
"#;
    assert_e1002(multi_src);
}

#[test]
fn array_literal_void_and_multivalue_rejected() {
    let void_src = r#"
fn void_fn() {}

fn main() {
    arr := [1]int{void_fn()}
    _ = arr
}
"#;
    assert_e1002(void_src);

    let multi_src = r#"
fn two_vals() (int, int) {
    return 1, 2
}

fn main() {
    arr := [1]int{two_vals()}
    _ = arr
}
"#;
    assert_e1002(multi_src);
}

#[test]
fn map_literal_key_and_value_void_and_multivalue_rejected() {
    let key_void = r#"
fn void_fn() {}

fn main() {
    m := map[int]int{void_fn(): 1}
    _ = m
}
"#;
    assert_e1002(key_void);

    let key_multi = r#"
fn two_vals() (int, int) {
    return 1, 2
}

fn main() {
    m := map[int]int{two_vals(): 1}
    _ = m
}
"#;
    assert_e1002(key_multi);

    let val_void = r#"
fn void_fn() {}

fn main() {
    m := map[int]int{1: void_fn()}
    _ = m
}
"#;
    assert_e1002(val_void);

    let val_multi = r#"
fn two_vals() (int, int) {
    return 1, 2
}

fn main() {
    m := map[int]int{1: two_vals()}
    _ = m
}
"#;
    assert_e1002(val_multi);
}

#[test]
fn struct_literal_field_void_and_multivalue_rejected() {
    let void_src = r#"
struct sample {
    x int
}

fn void_fn() {}

fn main() {
    s := sample{x: void_fn()}
    _ = s
}
"#;
    assert_e1002(void_src);

    let multi_src = r#"
struct sample {
    x int
}

fn two_vals() (int, int) {
    return 1, 2
}

fn main() {
    s := sample{x: two_vals()}
    _ = s
}
"#;
    assert_e1002(multi_src);
}

#[test]
fn nested_untyped_struct_literal_field_void_rejected() {
    let slice_nested = r#"
struct sample {
    x int
}

fn void_fn() {}

fn main() {
    xs := []sample{{x: void_fn()}}
    _ = xs
}
"#;
    assert_e1002(slice_nested);

    let struct_nested = r#"
struct inner {
    x int
}

struct outer {
    in inner
}

fn void_fn() {}

fn main() {
    o := outer{in: {x: void_fn()}}
    _ = o
}
"#;
    assert_e1002(struct_nested);
}

#[test]
fn nested_untyped_slice_literal_void_rejected() {
    let nested_slice = r#"
fn void_fn() {}

fn main() {
    matrix := [][]int{{void_fn()}}
    _ = matrix
}
"#;
    assert_e1002(nested_slice);
}

#[test]
fn valid_single_value_composite_literals_accepted() {
    let src = r#"
struct inner {
    x int
}

struct outer {
    in inner
}

fn single_val() int {
    return 42
}

fn main() {
    xs := []int{single_val()}
    arr := [1]int{single_val()}
    m := map[int]int{single_val(): single_val()}
    s := inner{x: single_val()}
    o := outer{in: inner{x: single_val()}}
    inners := []inner{{x: single_val()}}
    matrix := [][]int{{single_val()}}
    _ = xs
    _ = arr
    _ = m
    _ = s
    _ = o
    _ = inners
    _ = matrix
}
"#;
    assert_ok(src);
}

