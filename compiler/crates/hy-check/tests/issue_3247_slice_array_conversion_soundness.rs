//! issue_3247_slice_array_conversion_soundness.rs — regression tests for Issue #3247:
//! element scalar width and array length matching in slice and array type conversions.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("snippet must parse");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.to_string())
        .collect()
}

#[test]
fn test_issue_3247_array_conversion_length_mismatch_rejects() {
    let src = r#"
package main

fn main() {
    var a [3]int
    _ = [4]int{}(a)
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert_eq!(error_codes(&res), vec!["HYB-E1002"]);
}

#[test]
fn test_issue_3247_array_conversion_scalar_width_mismatch_rejects() {
    let src = r#"
package main

fn main() {
    var a [4]int64
    _ = [4]int32{}(a)
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert_eq!(error_codes(&res), vec!["HYB-E1002"]);
}

#[test]
fn test_issue_3247_array_conversion_element_array_length_mismatch_rejects() {
    let src = r#"
package main

fn main() {
    var a [2][3]int
    _ = [2][4]int{}(a)
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert_eq!(error_codes(&res), vec!["HYB-E1002"]);
}

#[test]
fn test_issue_3247_slice_to_array_conversion_scalar_mismatch_rejects() {
    let src = r#"
package main

fn main() {
    var xs []int64
    _ = [4]int32{}(xs)
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert_eq!(error_codes(&res), vec!["HYB-E1002"]);
}

#[test]
fn test_issue_3247_slice_to_array_conversion_element_array_length_mismatch_rejects() {
    let src = r#"
package main

fn main() {
    var xs [][3]int
    _ = [4][2]int{}(xs)
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert_eq!(error_codes(&res), vec!["HYB-E1002"]);
}

#[test]
fn test_issue_3247_slice_conversion_scalar_width_mismatch_rejects() {
    let src = r#"
package main

fn main() {
    var xs []int64
    _ = []int32(xs)
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert_eq!(error_codes(&res), vec!["HYB-E1002"]);
}

#[test]
fn test_issue_3247_slice_conversion_element_array_length_mismatch_rejects() {
    let src = r#"
package main

fn main() {
    var xs [][3]int
    _ = [][2]int(xs)
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert_eq!(error_codes(&res), vec!["HYB-E1002"]);
}

#[test]
fn test_issue_3247_named_array_alias_length_mismatch_rejects() {
    let src = r#"
package main

type arr4 = [4]int
type arr3 = [3]int

fn main() {
    var a arr3
    _ = arr4(a)
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert_eq!(error_codes(&res), vec!["HYB-E1002"]);
}

#[test]
fn test_issue_3247_named_slice_alias_scalar_mismatch_rejects() {
    let src = r#"
package main

type slice32 = []int32
type slice64 = []int64

fn main() {
    var xs slice64
    _ = slice32(xs)
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert_eq!(error_codes(&res), vec!["HYB-E1002"]);
}

#[test]
fn test_issue_3247_array_and_slice_conversions_matching_accepts() {
    let src = r#"
package main

type arr4 = [4]int
type slice32 = []int32

fn main() {
    var a [4]int
    _ = [4]int{}(a)

    var b arr4
    _ = arr4(b)

    var xs []int
    _ = [4]int{}(xs)

    var ys []int32
    _ = []int32(ys)

    var ys_named slice32
    _ = slice32(ys_named)

    var zs [][2]int
    _ = [][2]int(zs)
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Accept);
    assert_eq!(error_codes(&res), Vec::<String>::new());
}
