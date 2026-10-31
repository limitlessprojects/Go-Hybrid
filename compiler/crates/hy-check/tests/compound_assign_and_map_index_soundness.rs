use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> (Verdict, Vec<hy_diag::Diagnostic>) {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    (res.verdict, res.diagnostics)
}

#[test]
fn compound_assign_non_const_int_to_float_rejected() {
    let cases = [
        r#"
fn main() {
    var f float64 = 1.0
    var i int = 2
    f += i
}
"#,
        r#"
fn main() {
    var f float64 = 1.0
    var i int = 2
    f -= i
}
"#,
        r#"
fn main() {
    var f float64 = 1.0
    var i int = 2
    f *= i
}
"#,
        r#"
fn main() {
    var f float64 = 1.0
    var i int = 2
    f /= i
}
"#,
    ];

    for src in cases {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Reject,
            "expected reject for compound assign float-int mismatch:\n{src}"
        );
        assert!(
            diags.iter().any(|d| d.code == "HYB-E1002"),
            "expected HYB-E1002 in diags: {:?}",
            diags
        );
    }
}

#[test]
fn compound_assign_untyped_const_int_and_same_type_accepted() {
    let src = r#"
fn main() {
    var f float64 = 1.0
    var f2 float64 = 2.0
    f += 2
    f -= 1
    f *= 3
    f /= 2
    f += f2
}
"#;
    let (verdict, diags) = check(src);
    assert_eq!(
        verdict,
        Verdict::Accept,
        "expected accept for valid compound assignments: {:?}",
        diags
    );
}

#[test]
fn assign_non_const_int_to_float_rejected() {
    let cases = [
        // Single assign
        r#"
fn main() {
    var f float64 = 1.0
    var i int = 2
    f = i
}
"#,
        // Parallel assign
        r#"
fn main() {
    var f float64 = 1.0
    var i int = 2
    var s string = "a"
    f, s = i, "b"
}
"#,
    ];

    for src in cases {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Reject,
            "expected reject for assignment float-int mismatch:\n{src}"
        );
        assert!(
            diags.iter().any(|d| d.code == "HYB-E1002"),
            "expected HYB-E1002 in diags: {:?}",
            diags
        );
    }
}

#[test]
fn map_index_non_const_int_for_float_key_rejected() {
    let cases = [
        // Map read index
        r#"
fn main() {
    var i int = 1
    m := map[float64]string{}
    _ = m[i]
}
"#,
        // Map write index
        r#"
fn main() {
    var i int = 1
    m := map[float64]string{}
    m[i] = "hello"
}
"#,
    ];

    for src in cases {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Reject,
            "expected reject for map index key mismatch:\n{src}"
        );
        assert!(
            diags.iter().any(|d| d.code == "HYB-E1002"),
            "expected HYB-E1002 in diags: {:?}",
            diags
        );
    }
}

#[test]
fn map_index_untyped_const_int_and_same_type_accepted() {
    let src = r#"
fn main() {
    var f float64 = 1.0
    m := map[float64]string{}
    _ = m[1]
    _ = m[f]
    m[2] = "ok"
    m[f] = "ok2"
}
"#;
    let (verdict, diags) = check(src);
    assert_eq!(
        verdict,
        Verdict::Accept,
        "expected accept for valid map index expressions: {:?}",
        diags
    );
}
