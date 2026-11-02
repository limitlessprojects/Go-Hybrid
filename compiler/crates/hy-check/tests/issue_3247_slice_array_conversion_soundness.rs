//! Tests for #3247:
//! Type Soundness: Enforce Element Scalar Width and Array Length for Slice and Array Type Conversions.
//!
//! Reject array length mismatches (e.g. `arr4(a)` on `arr3`),
//! array element scalar width mismatches (e.g. `arr4(a)` on `arr4_64`),
//! and slice element scalar width mismatches (e.g. `my_int32s(s)` on `my_ints`, `[]int(s)` on `[]byte`)
//! with HYB-E1002.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> (Verdict, Vec<hy_diag::Diagnostic>) {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    (res.verdict, res.diagnostics)
}

#[test]
fn array_and_slice_conversion_mismatches_rejected_e1002() {
    let cases = [
        // 1. Array length mismatch: [4]int from [3]int
        (
            r#"
type arr3 = [3]int
type arr4 = [4]int
fn main() {
    var a arr3
    _ = arr4(a)
}
"#,
            "arr4 on arr3 (length mismatch)",
        ),
        // 2. Array element scalar width mismatch: [4]int from [4]int64
        (
            r#"
type arr4 = [4]int
type arr4_64 = [4]int64
fn main() {
    var a arr4_64
    _ = arr4(a)
}
"#,
            "arr4 on arr4_64 (element scalar width mismatch)",
        ),
        // 3. Named slice element scalar width mismatch: []int32 from []int64
        (
            r#"
type my_ints = []int64
type my_int32s = []int32
fn main() {
    var s my_ints
    _ = my_int32s(s)
}
"#,
            "my_int32s on my_ints (slice element scalar width mismatch)",
        ),
        // 4. Literal slice conversion with scalar mismatch: []int from []byte
        (
            r#"
fn main() {
    s := []byte("hello")
    _ = []int(s)
}
"#,
            "[]int on []byte",
        ),
        // 5. Literal slice conversion with scalar mismatch: []int from []rune
        (
            r#"
fn main() {
    s := []rune("hello")
    _ = []int(s)
}
"#,
            "[]int on []rune",
        ),
    ];

    for (src, desc) in cases {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Reject,
            "mismatched conversion case '{}' should be rejected, got Accept:\n{}",
            desc,
            src
        );
        let has_e1002 = diags.iter().any(|d| d.code == "HYB-E1002");
        assert!(
            has_e1002,
            "mismatched conversion case '{}' should emit HYB-E1002, got diags: {:?}\n{}",
            desc,
            diags,
            src
        );
    }
}

#[test]
fn array_and_slice_conversion_matching_accepted() {
    let cases = [
        // 1. Matching array lengths and element types
        r#"
type arr3 = [3]int
type arr3_alias = [3]int
fn main() {
    var a arr3
    _ = arr3_alias(a)
}
"#,
        // 2. Matching named slice to literal slice
        r#"
type my_bytes = []byte
fn main() {
    s := []byte("hello")
    _ = my_bytes(s)
}
"#,
        // 3. Matching slice to slice
        r#"
fn main() {
    s := []int{1, 2}
    _ = []int(s)
}
"#,
    ];

    for (idx, src) in cases.iter().enumerate() {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Accept,
            "matching conversion case #{} should accept, got Reject with diags: {:?}\n{}",
            idx + 1,
            diags,
            src
        );
    }
}
