//! Regression tests for #2538:
//! `copy(dst, src)` requires identical slice element types (including float64 vs int),
//! preventing mismatched slice element types from escaping to go build.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> (Verdict, Vec<hy_diag::Diagnostic>) {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    (res.verdict, res.diagnostics)
}

#[test]
fn copy_mismatched_slice_element_types_rejected_e1002() {
    let cases = [
        // 1. []float64 dst and []int src
        r#"
fn main() {
    var dst []float64 = []float64{1.0}
    var src []int = []int{1}
    copy(dst, src)
}
"#,
        // 2. []int dst and []float64 src
        r#"
fn main() {
    var dst []int = []int{1}
    var src []float64 = []float64{1.0}
    copy(dst, src)
}
"#,
        // 3. []any dst and []int src
        r#"
fn main() {
    var dst []any = []any{1}
    var src []int = []int{1}
    copy(dst, src)
}
"#,
        // 4. []string dst and []int src
        r#"
fn main() {
    var dst []string = []string{"a"}
    var src []int = []int{1}
    copy(dst, src)
}
"#,
    ];

    for (idx, src) in cases.iter().enumerate() {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Reject,
            "case #{idx} should reject, got Accept:\n{src}"
        );
        let has_e1002 = diags.iter().any(|d| d.code == "HYB-E1002");
        assert!(
            has_e1002,
            "case #{idx} should emit HYB-E1002, got: {diags:?}\n{src}"
        );
    }
}

#[test]
fn copy_identical_slice_element_types_accepted() {
    let cases = [
        // 1. []int and []int
        r#"
fn main() {
    var dst []int = []int{1, 2}
    var src []int = []int{3, 4}
    copy(dst, src)
}
"#,
        // 2. []float64 and []float64
        r#"
fn main() {
    var dst []float64 = []float64{1.0, 2.0}
    var src []float64 = []float64{3.0, 4.0}
    copy(dst, src)
}
"#,
        // 3. []byte (int) and string
        r#"
fn main() {
    var dst []byte = []byte{0, 0, 0}
    copy(dst, "abc")
}
"#,
        // 4. []string and []string
        r#"
fn main() {
    var dst []string = []string{"a"}
    var src []string = []string{"b"}
    copy(dst, src)
}
"#,
    ];

    for (idx, src) in cases.iter().enumerate() {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Accept,
            "case #{idx} should accept, got Reject with diags: {diags:?}\n{src}"
        );
    }
}
