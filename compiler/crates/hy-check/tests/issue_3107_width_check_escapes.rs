//! issue_3107_width_check_escapes.rs — tests for HYB-E1002 rejection
//! of escaped scalar width checks through len/cap/copy builtins, file-level
//! typed consts, and named-fn/method-value locals (fixes #3107).

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
fn builtin_len_cap_copy_scalar_widths_reject_e1002() {
    let cases = [
        // t1: len(slice) as i32
        "fn main() {\n    xs := []int{1}\n    var n i32 = len(xs)\n    _ = n\n}\n",
        // t2: cap(slice) as i64
        "fn main() {\n    xs := make([]int, 1, 4)\n    var n i64 = cap(xs)\n    _ = n\n}\n",
        // t3: copy(a, b) as u8
        "fn main() {\n    a := []int{1}\n    b := []int{2}\n    var n u8 = copy(a, b)\n    _ = n\n}\n",
        // t9: len(string) as i32
        "fn main() {\n    s := \"abc\"\n    var n i32 = len(s)\n    _ = n\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:\n{}", src);
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for:\n{}",
            src
        );
    }
}

#[test]
fn file_level_typed_consts_reject_e1002() {
    let cases = [
        // t4: file-level typed const k i8 assigned to i64
        "const k i8 = 3\nfn main() {\n    var y i64 = k\n    _ = y\n}\n",
        // t8: file-level typed const k i8 passed to param of type i64
        "const k i8 = 3\nfn take(v i64) { _ = v }\nfn main() {\n    take(k)\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:\n{}", src);
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for:\n{}",
            src
        );
    }
}

#[test]
fn named_fn_and_method_value_locals_reject_e1002() {
    let cases = [
        // t6: local bound to named fn get() i8 called and assigned to i64
        "fn get() i8 { return 1 }\nfn main() {\n    f := get\n    var y i64 = f()\n    _ = y\n}\n",
        // t7: local bound to method value a.get() i8 called and assigned to i64
        "struct c { n i8 }\nfn (x c) get() i8 { return x.n }\nfn main() {\n    a := c{n: 1}\n    f := a.get\n    var y i64 = f()\n    _ = y\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:\n{}", src);
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for:\n{}",
            src
        );
    }
}

#[test]
fn valid_control_accepts() {
    let valid = r#"
const k i8 = 3
fn get() i8 { return 1 }
fn main() {
    xs := []int{1}
    var n int = len(xs)
    var y i8 = k
    f := get
    var z i8 = f()
    var u i64 = 3
    _ = n
    _ = y
    _ = z
    _ = u
}
"#;
    let r = check(valid);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "valid program must pass check, got: {:?}",
        r.diagnostics
    );
    assert_eq!(error_codes(&r).len(), 0);
}
