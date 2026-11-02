//! Tests for #3242:
//! Type Soundness: Enforce Slice Element Scalar Width, Array Length, and Byte-Only Destination for Built-in `copy`.
//!
//! Reject slice element scalar width mismatches (e.g. `[]int32` vs `[]int64`),
//! fixed-size array length mismatches (e.g. `[][2]int` vs `[][3]int`),
//! and copying from string into non-byte slice (e.g. `[]int` vs string) with HYB-E1002.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> (Verdict, Vec<hy_diag::Diagnostic>) {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    (res.verdict, res.diagnostics)
}

#[test]
fn copy_slice_mismatches_rejected_e1002() {
    let cases = [
        // 1. []int32 dst and []int64 src
        (
            r#"
fn main() {
    var dst []int32
    var src []int64
    copy(dst, src)
}
"#,
            "[]int32 and []int64",
        ),
        // 2. []int dst and []int32 src
        (
            r#"
fn main() {
    var dst []int
    var src []int32
    copy(dst, src)
}
"#,
            "[]int and []int32",
        ),
        // 3. [][2]int dst and [][3]int src
        (
            r#"
fn main() {
    var dst [][2]int
    var src [][3]int
    copy(dst, src)
}
"#,
            "[][2]int and [][3]int",
        ),
        // 4. []int dst with string src
        (
            r#"
fn main() {
    var dst []int
    copy(dst, "hello")
}
"#,
            "[]int dst with string src",
        ),
        // 5. []int32 dst with string src
        (
            r#"
fn main() {
    var dst []int32
    copy(dst, "hello")
}
"#,
            "[]int32 dst with string src",
        ),
    ];

    for (src, desc) in cases {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Reject,
            "mismatched copy argument case '{}' should be rejected, got Accept:\n{}",
            desc,
            src
        );
        let has_e1002 = diags.iter().any(|d| d.code == "HYB-E1002");
        assert!(
            has_e1002,
            "mismatched copy argument case '{}' should emit HYB-E1002, got diags: {:?}\n{}",
            desc,
            diags,
            src
        );
    }
}

#[test]
fn copy_matching_types_accepted() {
    let cases = [
        // 1. matching []int32 slices
        r#"
fn main() {
    var dst []int32
    var src []int32
    copy(dst, src)
}
"#,
        // 2. matching [][2]int slices
        r#"
fn main() {
    var dst [][2]int
    var src [][2]int
    copy(dst, src)
}
"#,
        // 3. []byte dst with string src
        r#"
fn main() {
    var dst []byte
    copy(dst, "hello")
}
"#,
        // 4. []uint8 dst with string src
        r#"
fn main() {
    var dst []uint8
    copy(dst, "hello")
}
"#,
    ];

    for (idx, src) in cases.iter().enumerate() {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Accept,
            "matching copy argument case #{} should accept, got Reject with diags: {:?}\n{}",
            idx + 1,
            diags,
            src
        );
    }
}
