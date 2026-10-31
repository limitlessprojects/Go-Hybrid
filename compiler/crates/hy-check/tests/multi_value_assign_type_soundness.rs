//! multi_value_assign_type_soundness.rs — regression tests for multi-value assignment
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
    let src1 = "fn two() (int, string) {\n    return 1, \"hello\"\n}\n\nfn main() {\n    var a int\n    var b int\n    a, b = two()\n    _ = a\n    _ = b\n}\n";
    let r1 = check(src1);
    assert_eq!(r1.verdict, Verdict::Reject, "expected rejection when assigning string to int target in multi-return call");
    assert!(error_codes(&r1).contains(&"HYB-E1002".to_string()));

    let src2 = "fn three() (int, float64, string) {\n    return 1, 2.0, \"three\"\n}\n\nfn main() {\n    var a int\n    var b int\n    var c string\n    a, b, c = three()\n    _ = a\n    _ = b\n    _ = c\n}\n";
    let r2 = check(src2);
    assert_eq!(r2.verdict, Verdict::Reject, "expected rejection when assigning float64 to int target in 3-result call");
    assert!(error_codes(&r2).contains(&"HYB-E1002".to_string()));
}

#[test]
fn comma_ok_map_type_mismatch_rejected() {
    let src_ok_mismatch = "fn main() {\n    m := map[string]int{\"x\": 10}\n    var v int\n    var bad_ok string\n    v, bad_ok = m[\"x\"]\n    _ = v\n    _ = bad_ok\n}\n";
    let r1 = check(src_ok_mismatch);
    assert_eq!(r1.verdict, Verdict::Reject, "expected rejection when ok target in comma-ok map lookup is not bool");
    assert!(error_codes(&r1).contains(&"HYB-E1002".to_string()));

    let src_val_mismatch = "fn main() {\n    m := map[string]int{\"x\": 10}\n    var bad_v string\n    var ok bool\n    bad_v, ok = m[\"x\"]\n    _ = bad_v\n    _ = ok\n}\n";
    let r2 = check(src_val_mismatch);
    assert_eq!(r2.verdict, Verdict::Reject, "expected rejection when value target in comma-ok map lookup does not match int");
    assert!(error_codes(&r2).contains(&"HYB-E1002".to_string()));
}

#[test]
fn comma_ok_chan_type_mismatch_rejected() {
    let src_ok_mismatch = "fn main() {\n    ch := make(chan int, 1)\n    var cv int\n    var bad_ok int\n    cv, bad_ok = <-ch\n    _ = cv\n    _ = bad_ok\n}\n";
    let r1 = check(src_ok_mismatch);
    assert_eq!(r1.verdict, Verdict::Reject, "expected rejection when ok target in comma-ok channel receive is not bool");
    assert!(error_codes(&r1).contains(&"HYB-E1002".to_string()));

    let src_val_mismatch = "fn main() {\n    ch := make(chan int, 1)\n    var bad_cv string\n    var ok bool\n    bad_cv, ok = <-ch\n    _ = bad_cv\n    _ = ok\n}\n";
    let r2 = check(src_val_mismatch);
    assert_eq!(r2.verdict, Verdict::Reject, "expected rejection when value target in comma-ok channel receive does not match int");
    assert!(error_codes(&r2).contains(&"HYB-E1002".to_string()));
}

#[test]
fn valid_multi_value_assignments_accepted() {
    let src = "fn two() (int, string) {\n    return 1, \"ok\"\n}\n\nfn main() {\n    var a int\n    var s string\n    a, s = two()\n    _ = a\n    _ = s\n\n    var a2 int\n    a2, _ = two()\n    _ = a2\n\n    var s2 string\n    _, s2 = two()\n    _ = s2\n\n    m := map[string]int{\"k\": 42}\n    var mv int\n    var mok bool\n    mv, mok = m[\"k\"]\n    _ = mv\n    _ = mok\n\n    ch := make(chan int, 1)\n    var cv int\n    var cok bool\n    cv, cok = <-ch\n    _ = cv\n    _ = cok\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected valid matching multi-value assignments to accept cleanly");
    assert!(error_codes(&r).is_empty(), "expected 0 errors, got: {:?}", r.diagnostics);
}
