//! Regression tests for issue #3280:
//! hy-check: builtin append fails to verify fixed-size array dimension and nested slice element scalar soundness (HYB-E1002)

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).unwrap_or_else(|d| panic!("must parse: {d:?}"));
    check_file(&ast)
}

fn errors(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect()
}

#[test]
fn test_append_array_dimension_mismatch_rejected() {
    let src = r#"package main

fn main() {
    var s [][2]int
    var a [3]int
    s = append(s, a)
}
"#;
    let r = check(src);
    let errs = errors(&r);
    assert_eq!(r.verdict, Verdict::Reject, "must reject array dimension mismatch in append");
    assert!(
        errs.iter().any(|e| e.starts_with("HYB-E1002")),
        "expected HYB-E1002, got: {:?}",
        errs
    );
}

#[test]
fn test_append_array_literal_dimension_mismatch_rejected() {
    let src = r#"package main

fn main() {
    var s [][2]int
    s = append(s, [3]int{1, 2, 3})
}
"#;
    let r = check(src);
    let errs = errors(&r);
    assert_eq!(r.verdict, Verdict::Reject, "must reject array literal dimension mismatch in append");
    assert!(
        errs.iter().any(|e| e.starts_with("HYB-E1002")),
        "expected HYB-E1002, got: {:?}",
        errs
    );
}

#[test]
fn test_append_nested_slice_scalar_mismatch_rejected() {
    let src = r#"package main

fn main() {
    var s [][]int64
    var item []int32
    s = append(s, item)
}
"#;
    let r = check(src);
    let errs = errors(&r);
    assert_eq!(r.verdict, Verdict::Reject, "must reject nested slice scalar mismatch in append");
    assert!(
        errs.iter().any(|e| e.starts_with("HYB-E1002")),
        "expected HYB-E1002, got: {:?}",
        errs
    );
}

#[test]
fn test_append_nested_slice_literal_scalar_mismatch_rejected() {
    let src = r#"package main

fn main() {
    var s [][]int64
    s = append(s, []int32{1, 2})
}
"#;
    let r = check(src);
    let errs = errors(&r);
    assert_eq!(r.verdict, Verdict::Reject, "must reject nested slice literal scalar mismatch in append");
    assert!(
        errs.iter().any(|e| e.starts_with("HYB-E1002")),
        "expected HYB-E1002, got: {:?}",
        errs
    );
}

#[test]
fn test_append_array_spread_dimension_mismatch_rejected() {
    let src = r#"package main

fn main() {
    var s [][2]int
    var b [][3]int
    s = append(s, b...)
}
"#;
    let r = check(src);
    let errs = errors(&r);
    assert_eq!(r.verdict, Verdict::Reject, "must reject array spread dimension mismatch in append");
    assert!(
        errs.iter().any(|e| e.starts_with("HYB-E1002")),
        "expected HYB-E1002, got: {:?}",
        errs
    );
}

#[test]
fn test_append_nested_slice_spread_scalar_mismatch_rejected() {
    let src = r#"package main

fn main() {
    var s [][]int64
    var items [][]int32
    s = append(s, items...)
}
"#;
    let r = check(src);
    let errs = errors(&r);
    assert_eq!(r.verdict, Verdict::Reject, "must reject nested slice spread scalar mismatch in append");
    assert!(
        errs.iter().any(|e| e.starts_with("HYB-E1002")),
        "expected HYB-E1002, got: {:?}",
        errs
    );
}

#[test]
fn test_append_struct_field_array_dimension_mismatch_rejected() {
    let src = r#"package main

struct container {
    matrix [][2]int
    bad_arr [3]int
}

fn main() {
    var c container
    c.matrix = append(c.matrix, c.bad_arr)
}
"#;
    let r = check(src);
    let errs = errors(&r);
    assert_eq!(r.verdict, Verdict::Reject, "must reject struct field array dimension mismatch in append");
    assert!(
        errs.iter().any(|e| e.starts_with("HYB-E1002")),
        "expected HYB-E1002, got: {:?}",
        errs
    );
}

#[test]
fn test_append_struct_field_nested_slice_scalar_mismatch_rejected() {
    let src = r#"package main

struct container {
    grid [][]int64
    bad_slice []int32
}

fn main() {
    var c container
    c.grid = append(c.grid, c.bad_slice)
}
"#;
    let r = check(src);
    let errs = errors(&r);
    assert_eq!(r.verdict, Verdict::Reject, "must reject struct field nested slice scalar mismatch in append");
    assert!(
        errs.iter().any(|e| e.starts_with("HYB-E1002")),
        "expected HYB-E1002, got: {:?}",
        errs
    );
}

#[test]
fn test_append_type_aliased_array_and_slice_rejected() {
    let src = r#"package main

type row = [2]int
type bad_row = [3]int

fn main() {
    var s []row
    var r bad_row
    s = append(s, r)
}
"#;
    let r = check(src);
    let errs = errors(&r);
    assert_eq!(r.verdict, Verdict::Reject, "must reject type-aliased array dimension mismatch in append");
    assert!(
        errs.iter().any(|e| e.starts_with("HYB-E1002")),
        "expected HYB-E1002, got: {:?}",
        errs
    );
}

#[test]
fn test_valid_appends_accepted() {
    let src = r#"package main

struct container {
    matrix [][2]int
    good_arr [2]int
}

fn main() {
    var s [][2]int
    var a [2]int
    s = append(s, a)

    var s2 [][2]int
    var b [][2]int
    s2 = append(s2, b...)

    var s3 [][]int64
    var item []int64
    s3 = append(s3, item)

    var s4 []int
    s4 = append(s4, 1, 2, 3)

    var c container
    c.matrix = append(c.matrix, c.good_arr)
}
"#;
    let r = check(src);
    let errs = errors(&r);
    assert_eq!(r.verdict, Verdict::Accept, "valid appends must be accepted, got: {:?}", errs);
}
