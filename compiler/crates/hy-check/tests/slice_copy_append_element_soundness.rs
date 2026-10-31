use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> (Verdict, Vec<hy_diag::Diagnostic>) {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    (res.verdict, res.diagnostics)
}

#[test]
fn copy_mismatched_element_types_rejected() {
    let cases = [
        // float64 vs int
        r#"
fn main() {
    dst := []float64{1.0, 2.0}
    src := []int{1, 2}
    copy(dst, src)
}
"#,
        // int vs string
        r#"
fn main() {
    dst := []int{1, 2}
    src := []string{"a", "b"}
    copy(dst, src)
}
"#,
        // string into non-byte slice
        r#"
fn main() {
    dst := []float64{1.0, 2.0}
    copy(dst, "hello")
}
"#,
    ];

    for src in cases {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Reject,
            "expected reject for mismatched copy elements:\n{src}"
        );
        assert!(
            diags.iter().any(|d| d.code == "HYB-E1002"),
            "expected HYB-E1002 in diags: {:?}",
            diags
        );
    }
}

#[test]
fn copy_identical_element_types_accepted() {
    let src = r#"
fn main() {
    dst := []int{1, 2}
    src := []int{3, 4}
    copy(dst, src)
}
"#;
    let (verdict, diags) = check(src);
    assert_eq!(verdict, Verdict::Accept, "expected accept for identical copy elements: {:?}", diags);
}

#[test]
fn append_non_const_int_to_float_and_void_rejected() {
    let cases = [
        // non-constant int variable into float64 slice
        r#"
fn main() {
    var x int = 1
    xs := []float64{1.0}
    _ = append(xs, x)
}
"#,
        // void value into append
        r#"
fn void() {}

fn main() {
    xs := []int{1}
    _ = append(xs, void())
}
"#,
        // string into int slice
        r#"
fn main() {
    xs := []int{1}
    _ = append(xs, "hello")
}
"#,
    ];

    for src in cases {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Reject,
            "expected reject for append element mismatch:\n{src}"
        );
        assert!(
            diags.iter().any(|d| d.code == "HYB-E1002"),
            "expected HYB-E1002 in diags: {:?}",
            diags
        );
    }
}

#[test]
fn append_untyped_const_int_to_float_accepted() {
    let src = r#"
fn main() {
    xs := []float64{1.0}
    _ = append(xs, 2)
}
"#;
    let (verdict, diags) = check(src);
    assert_eq!(
        verdict,
        Verdict::Accept,
        "expected accept for untyped const int to float slice append: {:?}",
        diags
    );
}

#[test]
fn append_variadic_spread_mismatches_rejected() {
    let cases = [
        // int slice append string slice spread
        r#"
fn main() {
    xs := []int{1}
    ys := []string{"a", "b"}
    _ = append(xs, ys...)
}
"#,
        // float64 slice append int slice spread
        r#"
fn main() {
    xs := []float64{1.0}
    ys := []int{1, 2}
    _ = append(xs, ys...)
}
"#,
        // append non-slice with spread
        r#"
fn main() {
    xs := []int{1}
    _ = append(xs, 123...)
}
"#,
        // string spread into non-byte slice
        r#"
fn main() {
    xs := []float64{1.0}
    _ = append(xs, "str"...)
}
"#,
    ];

    for src in cases {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Reject,
            "expected reject for invalid variadic append spread:\n{src}"
        );
        assert!(
            diags.iter().any(|d| d.code == "HYB-E1002"),
            "expected HYB-E1002 in diags: {:?}",
            diags
        );
    }
}

#[test]
fn append_variadic_spread_matching_slice_accepted() {
    let src = r#"
fn main() {
    xs := []int{1}
    ys := []int{2, 3}
    _ = append(xs, ys...)
}
"#;
    let (verdict, diags) = check(src);
    assert_eq!(
        verdict,
        Verdict::Accept,
        "expected accept for matching slice variadic append: {:?}",
        diags
    );
}
