//! issue_2495_multi_value_assign_soundness.rs — regression tests for multi-value assignment
//! target type soundness under Row 13 (BUILDABLE) and HYB-E1002 (issue #2495).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn multi_return_call_type_mismatch_rejected() {
    let src1 = r#"
fn two() (int, string) {
    return 1, "hello"
}

fn main() {
    var a int
    var b int
    a, b = two()
    _ = a
    _ = b
}
"#;
    let r1 = check(src1);
    assert_eq!(
        r1.verdict,
        Verdict::Reject,
        "expected rejection when assigning string to int target in multi-return call"
    );
    assert!(error_codes(&r1).contains(&"HYB-E1002".to_string()));

    let src2 = r#"
fn three() (int, float64, string) {
    return 1, 2.0, "three"
}

fn main() {
    var a int
    var b int
    var c string
    a, b, c = three()
    _ = a
    _ = b
    _ = c
}
"#;
    let r2 = check(src2);
    assert_eq!(
        r2.verdict,
        Verdict::Reject,
        "expected rejection when assigning float64 to int target in 3-result call"
    );
    assert!(error_codes(&r2).contains(&"HYB-E1002".to_string()));
}

#[test]
fn multi_return_call_float_int_mismatch_rejected() {
    let src_int_to_float = r#"
fn two_ints() (int, int) {
    return 1, 2
}

fn main() {
    var f float64
    var b int
    f, b = two_ints()
    _ = f
    _ = b
}
"#;
    let r1 = check(src_int_to_float);
    assert_eq!(
        r1.verdict,
        Verdict::Reject,
        "expected rejection when assigning typed int return to float64 target in multi-return call"
    );
    assert!(error_codes(&r1).contains(&"HYB-E1002".to_string()));

    let src_float_to_int = r#"
fn two_floats() (float64, float64) {
    return 1.0, 2.0
}

fn main() {
    var a int
    var b float64
    a, b = two_floats()
    _ = a
    _ = b
}
"#;
    let r2 = check(src_float_to_int);
    assert_eq!(
        r2.verdict,
        Verdict::Reject,
        "expected rejection when assigning typed float64 return to int target in multi-return call"
    );
    assert!(error_codes(&r2).contains(&"HYB-E1002".to_string()));
}

#[test]
fn struct_field_target_multi_value_mismatch_rejected() {
    let src = r#"
struct stats {
    count int
    label int
}

fn two() (int, string) {
    return 1, "done"
}

fn main() {
    var st stats
    st.count, st.label = two()
    _ = st
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected rejection when assigning string return to int struct field target"
    );
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn comma_ok_map_type_mismatch_rejected() {
    let src_ok_mismatch = r#"
fn main() {
    m := map[string]int{"x": 10}
    var v int
    var bad_ok string
    v, bad_ok = m["x"]
    _ = v
    _ = bad_ok
}
"#;
    let r1 = check(src_ok_mismatch);
    assert_eq!(
        r1.verdict,
        Verdict::Reject,
        "expected rejection when ok target in comma-ok map lookup is not bool"
    );
    assert!(error_codes(&r1).contains(&"HYB-E1002".to_string()));

    let src_val_mismatch = r#"
fn main() {
    m := map[string]int{"x": 10}
    var bad_v string
    var ok bool
    bad_v, ok = m["x"]
    _ = bad_v
    _ = ok
}
"#;
    let r2 = check(src_val_mismatch);
    assert_eq!(
        r2.verdict,
        Verdict::Reject,
        "expected rejection when value target in comma-ok map lookup does not match int"
    );
    assert!(error_codes(&r2).contains(&"HYB-E1002".to_string()));
}

#[test]
fn comma_ok_chan_type_mismatch_rejected() {
    let src_ok_mismatch = r#"
fn main() {
    ch := make(chan int, 1)
    var cv int
    var bad_ok int
    cv, bad_ok = <-ch
    _ = cv
    _ = bad_ok
}
"#;
    let r1 = check(src_ok_mismatch);
    assert_eq!(
        r1.verdict,
        Verdict::Reject,
        "expected rejection when ok target in comma-ok channel receive is not bool"
    );
    assert!(error_codes(&r1).contains(&"HYB-E1002".to_string()));

    let src_val_mismatch = r#"
fn main() {
    ch := make(chan int, 1)
    var bad_cv string
    var ok bool
    bad_cv, ok = <-ch
    _ = bad_cv
    _ = ok
}
"#;
    let r2 = check(src_val_mismatch);
    assert_eq!(
        r2.verdict,
        Verdict::Reject,
        "expected rejection when value target in comma-ok channel receive does not match int"
    );
    assert!(error_codes(&r2).contains(&"HYB-E1002".to_string()));
}

#[test]
fn valid_multi_value_assignments_accepted() {
    let src = r#"
struct stats {
    count int
    label string
}

fn two() (int, string) {
    return 1, "ok"
}

fn two_ints() (int, int) {
    return 1, 2
}

fn main() {
    var a int
    var s string
    a, s = two()
    _ = a
    _ = s

    var a2 int
    a2, _ = two()
    _ = a2

    var s2 string
    _, s2 = two()
    _ = s2

    var x int
    var y int
    x, y = two_ints()
    _ = x
    _ = y

    var st stats
    st.count, st.label = two()
    _ = st

    m := map[string]int{"k": 42}
    var mv int
    var mok bool
    mv, mok = m["k"]
    _ = mv
    _ = mok

    var mv2 int
    mv2, _ = m["k"]
    _ = mv2

    var mok2 bool
    _, mok2 = m["k"]
    _ = mok2

    ch := make(chan int, 1)
    var cv int
    var cok bool
    cv, cok = <-ch
    _ = cv
    _ = cok

    var cv2 int
    cv2, _ = <-ch
    _ = cv2

    var cok2 bool
    _, cok2 = <-ch
    _ = cok2
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected valid matching multi-value assignments to accept cleanly"
    );
    assert!(
        error_codes(&r).is_empty(),
        "expected 0 errors, got: {:?}",
        r.diagnostics
    );
}
