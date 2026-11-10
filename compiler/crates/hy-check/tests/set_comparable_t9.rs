//! Regression tests for #2468: SPEC §2.3 T9 requires `set[T]` element types
//! to be comparable. Incomparable element types (slices, maps, functions,
//! data-carrying enums, and structs containing them) must be rejected with HYB-E1002.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn assert_reject_e1002(src: &str) {
    let ast = parse_file("test.hy", src).expect("parses");
    let res = check_file(&ast);
    assert_eq!(
        res.verdict,
        Verdict::Reject,
        "expected rejection with HYB-E1002, but accepted: {:?}",
        res.diagnostics
    );
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "expected HYB-E1002 diagnostic code, got: {:?}",
        res.diagnostics
    );
}

fn assert_accept(src: &str) {
    let ast = parse_file("test.hy", src).expect("parses");
    let res = check_file(&ast);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "expected acceptance, but rejected with: {:?}",
        res.diagnostics
    );
}

#[test]
fn set_slice_elem_rejects() {
    let src = "fn main() {
    s := set[[]int]{}
    fmt.println(s)
}
";
    assert_reject_e1002(src);
}

#[test]
fn set_map_elem_rejects() {
    let src = "fn main() {
    s := set[map[string]int]{}
    fmt.println(s)
}
";
    assert_reject_e1002(src);
}

#[test]
fn set_data_carrying_enum_elem_rejects() {
    let src = "enum shape {
    circle(f64)
    point
}
fn main() {
    s := set[shape]{}
    fmt.println(s)
}
";
    assert_reject_e1002(src);
}

#[test]
fn set_struct_with_slice_elem_rejects() {
    let src = "struct holder {
    items []int
}
fn main() {
    s := set[holder]{}
    fmt.println(s)
}
";
    assert_reject_e1002(src);
}

#[test]
fn set_comparable_types_accept() {
    let src = "enum mode {
    read
    write
}
struct point {
    x int
    y int
}
fn main() {
    s1 := set[int]{}
    s2 := set[string]{}
    s3 := set[mode]{}
    s4 := set[point]{}
    fmt.println(s1, s2, s3, s4)
}
";
    assert_accept(src);
}
