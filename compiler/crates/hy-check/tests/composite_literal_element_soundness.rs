use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> (Verdict, Vec<hy_diag::Diagnostic>) {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    (res.verdict, res.diagnostics)
}

#[test]
fn composite_literal_non_const_int_for_float_rejected() {
    let cases = [
        // Slice literal
        r#"
fn main() {
    var i int = 1
    xs := []float64{i}
    _ = xs
}
"#,
        // Array literal
        r#"
fn main() {
    var i int = 1
    arr := [1]float64{i}
    _ = arr
}
"#,
        // Map key
        r#"
fn main() {
    var i int = 1
    m := map[float64]string{i: "hello"}
    _ = m
}
"#,
        // Map value
        r#"
fn main() {
    var i int = 1
    m := map[string]float64{"hello": i}
    _ = m
}
"#,
    ];

    for src in cases {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Reject,
            "expected reject for non-const int in float composite literal:\n{src}"
        );
        assert!(
            diags.iter().any(|d| d.code == "HYB-E1002"),
            "expected HYB-E1002 in diags: {:?}",
            diags
        );
    }
}

#[test]
fn composite_literal_untyped_const_int_and_same_type_accepted() {
    let src = r#"
const c = 2

fn main() {
    var f float64 = 1.0

    // Slice literals
    s1 := []float64{1}
    s2 := []float64{c}
    s3 := []float64{f}
    _ = s1
    _ = s2
    _ = s3

    // Array literals
    a1 := [1]float64{1}
    a2 := [1]float64{c}
    a3 := [1]float64{f}
    _ = a1
    _ = a2
    _ = a3

    // Map keys
    mk1 := map[float64]string{1: "a"}
    mk2 := map[float64]string{c: "a"}
    mk3 := map[float64]string{f: "a"}
    _ = mk1
    _ = mk2
    _ = mk3

    // Map values
    mv1 := map[string]float64{"a": 1}
    mv2 := map[string]float64{"a": c}
    mv3 := map[string]float64{"a": f}
    _ = mv1
    _ = mv2
    _ = mv3
}
"#;
    let (verdict, diags) = check(src);
    assert_eq!(
        verdict,
        Verdict::Accept,
        "expected accept for valid composite literals: {:?}",
        diags
    );
}
