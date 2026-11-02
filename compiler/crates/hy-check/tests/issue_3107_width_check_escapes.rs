//! issue_3107_width_check_escapes.rs — tests for HYB-E1002 rejection
//! of mismatched typed integer widths from len/cap/copy, file-level typed consts,
//! and named-fn/method-value locals (fixes #3107).

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
fn len_cap_copy_rejects_mismatched_scalar_width() {
    let cases = [
        // t1: len(slice) gives int, rejected for i32
        "fn main() {\n    xs := []int{1}\n    var n i32 = len(xs)\n    _ = n\n}\n",
        // t2: cap(slice) gives int, rejected for i64
        "fn main() {\n    xs := make([]int, 1, 4)\n    var n i64 = cap(xs)\n    _ = n\n}\n",
        // t3: copy(dst, src) gives int, rejected for u8
        "fn main() {\n    a := []int{1}\n    b := []int{2}\n    var n u8 = copy(a, b)\n    _ = n\n}\n",
        // t9: len(string) gives int, rejected for i32
        "fn main() {\n    s := \"abc\"\n    var n i32 = len(s)\n    _ = n\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected rejection for:\n{}",
            src
        );
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 in {:?} for:\n{}",
            error_codes(&r),
            src
        );
    }
}

#[test]
fn file_level_typed_const_rejects_mismatched_scalar_width() {
    let cases = [
        // t4: const k i8 = 3, var y i64 = k
        "const k i8 = 3\nfn main() {\n    var y i64 = k\n    _ = y\n}\n",
        // t8: const k i8 = 3, take(k) where take accepts i64
        "const k i8 = 3\nfn take(v i64) { _ = v }\nfn main() {\n    take(k)\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected rejection for:\n{}",
            src
        );
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 in {:?} for:\n{}",
            error_codes(&r),
            src
        );
    }
}

#[test]
fn fn_and_method_value_locals_reject_mismatched_scalar_width() {
    let cases = [
        // t6: named function local f := get where get returns i8, rejected for i64
        "fn get() i8 { return 1 }\nfn main() {\n    f := get\n    var y i64 = f()\n    _ = y\n}\n",
        // t7: method value local f := a.get where get returns i8, rejected for i64
        "struct c { n i8 }\nfn (x c) get() i8 { return x.n }\nfn main() {\n    a := c{n: 1}\n    f := a.get\n    var y i64 = f()\n    _ = y\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected rejection for:\n{}",
            src
        );
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 in {:?} for:\n{}",
            error_codes(&r),
            src
        );
    }
}

#[test]
fn matching_scalar_widths_accept_as_valid() {
    let src = "const k i8 = 3\nfn get() i8 { return 1 }\nfn main() {\n    xs := []int{1}\n    var n int = len(xs)\n    var y i8 = k\n    f := get\n    var z i8 = f()\n    var u i64 = 3\n    _ = n\n    _ = y\n    _ = z\n    _ = u\n}\n";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "control program must accept cleanly, got: {:?}",
        r.diagnostics
    );
    assert!(
        r.diagnostics.is_empty(),
        "expected 0 diagnostics, got {:?}",
        r.diagnostics
    );
}
