//! Test suite for Issue #2476:
//! Soundness: for i in lo..hi accepts inferred float/bool/string non-integer range bounds violating LX13 (HYB-E2009).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("parses");
    check_file(&ast)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn assert_e2009(src: &str) {
    let cs = codes(src);
    assert!(
        cs.contains(&"HYB-E2009".to_string()),
        "expected HYB-E2009 in {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let r = check_file(&parse_file("test.hy", src).expect("parses"));
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected Verdict::Accept, got diagnostics: {:?}",
        r.diagnostics
    );
}

#[test]
fn test_reproduce_issue_2476_inferred_float_bound() {
    let src = r#"
fn main() {
    f := 3.14
    for i in 0..f {
        _ = i
    }
}
"#;
    assert_e2009(src);
}

#[test]
fn test_inferred_bool_bound() {
    let src = r#"
fn main() {
    b := true
    for i in 0..b {
        _ = i
    }
}
"#;
    assert_e2009(src);
}

#[test]
fn test_inferred_string_bound() {
    let src = r#"
fn main() {
    s := "hello"
    for i in 0..s {
        _ = i
    }
}
"#;
    assert_e2009(src);
}

#[test]
fn test_inferred_non_integer_binary_bound() {
    let src = r#"
fn main() {
    f1 := 1.0
    f2 := 2.0
    for i in 0..(f1 + f2) {
        _ = i
    }
}
"#;
    assert_e2009(src);
}

#[test]
fn test_inferred_non_integer_lower_bound() {
    let src = r#"
fn main() {
    f := 0.5
    for i in f..10 {
        _ = i
    }
}
"#;
    assert_e2009(src);
}

#[test]
fn test_inferred_inclusive_non_integer_bound() {
    let src = r#"
fn main() {
    f := 5.5
    for i in 0..=f {
        _ = i
    }
}
"#;
    assert_e2009(src);
}

#[test]
fn test_call_float_conversion_and_fn_return() {
    let src1 = r#"
fn main() {
    for i in 0..float64(10) {
        _ = i
    }
}
"#;
    assert_e2009(src1);

    let src2 = r#"
fn get_limit() float64 {
    return 10.5
}

fn main() {
    for i in 0..get_limit() {
        _ = i
    }
}
"#;
    assert_e2009(src2);
}

#[test]
fn test_positive_controls_valid_integer_bounds() {
    let src1 = r#"
fn main() {
    n := 10
    for i in 0..n {
        _ = i
    }
}
"#;
    assert_ok(src1);

    let src2 = r#"
fn main() {
    for i in 0..10 {
        _ = i
    }
}
"#;
    assert_ok(src2);

    let src3 = r#"
fn main() {
    var limit int = 20
    for i in 0..limit {
        _ = i
    }
}
"#;
    assert_ok(src3);

    let src4 = r#"
const max_iter = 100
fn main() {
    for i in 0..max_iter {
        _ = i
    }
}
"#;
    assert_ok(src4);

    let src5 = r#"
fn main() {
    xs := []int{1, 2, 3}
    for i in 0..len(xs) {
        _ = i
    }
}
"#;
    assert_ok(src5);

    let src6 = r#"
fn main() {
    n := 5
    for i in 0..=n {
        _ = i
    }
}
"#;
    assert_ok(src6);
}
