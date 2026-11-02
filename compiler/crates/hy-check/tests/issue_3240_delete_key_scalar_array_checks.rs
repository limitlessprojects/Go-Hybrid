//! Tests for #3240:
//! Type Soundness: Enforce Scalar Numeric Width and Sized Array Matching for Built-in `delete(m, k)` Arguments.
//!
//! Reject scalar width mismatches (e.g. `int64` key on `map[int32]string` or `set[int32]`)
//! and fixed-size array length mismatches (e.g. `[3]int` key on `map[[2]int]string` or `set[[2]int]`)
//! with HYB-E2005.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> (Verdict, Vec<hy_diag::Diagnostic>) {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    (res.verdict, res.diagnostics)
}

#[test]
fn delete_key_scalar_array_mismatches_rejected_e2005() {
    let cases = [
        // 1. map[int32]string with int64 variable key
        (
            r#"
fn main() {
    var m map[int32]string
    var k int64 = 1
    delete(m, k)
}
"#,
            "map[int32] with int64 key",
        ),
        // 2. map[uint8]string with int variable key
        (
            r#"
fn main() {
    var m map[uint8]string
    var k int = 1
    delete(m, k)
}
"#,
            "map[uint8] with int key",
        ),
        // 3. map[[2]int]string with [3]int variable key
        (
            r#"
fn main() {
    var m map[[2]int]string
    var k [3]int
    delete(m, k)
}
"#,
            "map[[2]int] with [3]int key",
        ),
        // 4. set[int32] with int64 variable element
        (
            r#"
fn main() {
    var s set[int32]
    var k int64 = 1
    delete(s, k)
}
"#,
            "set[int32] with int64 element",
        ),
        // 5. set[uint8] with int variable element
        (
            r#"
fn main() {
    var s set[uint8]
    var k int = 1
    delete(s, k)
}
"#,
            "set[uint8] with int element",
        ),
        // 6. set[[2]int] with [3]int variable element
        (
            r#"
fn main() {
    var s set[[2]int]
    var k [3]int
    delete(s, k)
}
"#,
            "set[[2]int] with [3]int element",
        ),
    ];

    for (src, desc) in cases {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Reject,
            "mismatched delete argument case '{}' should be rejected, got Accept:\n{}",
            desc,
            src
        );
        let has_e2005 = diags.iter().any(|d| d.code == "HYB-E2005");
        assert!(
            has_e2005,
            "mismatched delete argument case '{}' should emit HYB-E2005, got diags: {:?}\n{}",
            desc,
            diags,
            src
        );
    }
}

#[test]
fn delete_key_matching_types_accepted() {
    let cases = [
        // 1. map[int32]string with int32 variable key
        r#"
fn main() {
    var m map[int32]string
    var k int32 = 1
    delete(m, k)
}
"#,
        // 2. map[int32]string with untyped int literal key
        r#"
fn main() {
    var m map[int32]string
    delete(m, 1)
}
"#,
        // 3. map[[2]int]string with [2]int variable key
        r#"
fn main() {
    var m map[[2]int]string
    var k [2]int
    delete(m, k)
}
"#,
        // 4. set[int32] with int32 variable element
        r#"
fn main() {
    var s set[int32]
    var k int32 = 1
    delete(s, k)
}
"#,
        // 5. set[int32] with untyped int literal element
        r#"
fn main() {
    var s set[int32]
    delete(s, 1)
}
"#,
        // 6. set[[2]int] with [2]int variable element
        r#"
fn main() {
    var s set[[2]int]
    var k [2]int
    delete(s, k)
}
"#,
    ];

    for (idx, src) in cases.iter().enumerate() {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Accept,
            "matching delete argument case #{} should accept, got Reject with diags: {:?}\n{}",
            idx + 1,
            diags,
            src
        );
    }
}
