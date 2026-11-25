//! Regression test for Issue #2643:
//! Arbitrary pointer-to-pointer conversions escape validation (GUARANTEES.md Row 13).
//! Under Go language specification (§Conversions), converting a pointer of type `*T1`
//! to a pointer type `*T2` without `unsafe.Pointer` requires that `T1` and `T2` have
//! identical underlying types.

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
        "expected HYB-E1002 among diagnostics, got {cs:?} for:\n{src}"
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
fn pointer_conversion_between_incompatible_scalars_rejected() {
    let int_to_float = r#"
type float_ptr = *float64

fn main() {
    i := 1
    pi := &i
    pf := float_ptr(pi)
    _ = pf
}
"#;
    assert_e1002(int_to_float);

    let int_to_string = r#"
type string_ptr = *string

fn main() {
    i := 1
    pi := &i
    ps := string_ptr(pi)
    _ = ps
}
"#;
    assert_e1002(int_to_string);
}

#[test]
fn pointer_conversion_between_distinct_structs_rejected() {
    let struct_src = r#"
struct point_a {
    x int
}

struct point_b {
    y int
}

type point_b_ptr = *point_b

fn main() {
    a := point_a{x: 10}
    pa := &a
    pb := point_b_ptr(pa)
    _ = pb
}
"#;
    assert_e1002(struct_src);
}

#[test]
fn pointer_conversion_between_scalar_and_struct_rejected() {
    let src = r#"
struct payload {
    val int
}

type payload_ptr = *payload

fn main() {
    i := 100
    pi := &i
    pp := payload_ptr(pi)
    _ = pp
}
"#;
    assert_e1002(src);
}

#[test]
fn valid_pointer_conversions_accepted() {
    let src = r#"
type my_int = int
type my_int_ptr = *my_int

struct node {
    val int
}

type node_ptr = *node

fn main() {
    i := 42
    pi := &i
    p_my := my_int_ptr(pi)
    p_any := any(pi)

    n := node{val: 1}
    pn := &n
    pn2 := node_ptr(pn)

    _ = p_my
    _ = p_any
    _ = pn2
}
"#;
    assert_ok(src);
}
