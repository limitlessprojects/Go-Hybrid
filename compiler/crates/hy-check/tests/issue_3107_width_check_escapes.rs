//! issue_3107_width_check_escapes.rs — tests for HYB-E1002 typed-width check
//! escapes: builtin len/cap/copy results, file-level typed consts, and named-fn
//! or method-value locals (fixes #3107).

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
fn builtin_len_cap_copy_rejects_mismatched_width_e1002() {
    let cases = [
        // t1: len(xs) assigned to i32
        "fn main() {\n    xs := []int{1}\n    var n i32 = len(xs)\n    fmt.println(n)\n}\n",
        // t2: cap(xs) assigned to i64
        "fn main() {\n    xs := make([]int, 1, 4)\n    var n i64 = cap(xs)\n    fmt.println(n)\n}\n",
        // t3: copy(a, b) assigned to u8
        "fn main() {\n    a := []int{1}\n    b := []int{2}\n    var n u8 = copy(a, b)\n    fmt.println(n)\n}\n",
        // t9: len(s) on string assigned to i32
        "fn main() {\n    s := \"abc\"\n    var n i32 = len(s)\n    fmt.println(n)\n}\n",
        // len(xs) reassignment to mismatched local
        "fn main() {\n    xs := []int{1, 2}\n    var n i16 = 0\n    n = len(xs)\n    fmt.println(n)\n}\n",
        // len(xs) passed to fn expecting i64
        "fn take(v i64) { fmt.println(v) }\nfn main() {\n    xs := []int{1}\n    take(len(xs))\n}\n",
        // len(xs) binary op with mismatched integer type
        "fn main() {\n    xs := []int{1}\n    var a i32 = 1\n    c := a + len(xs)\n    fmt.println(c)\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:\n{}", src);
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002, got {:?} for:\n{}",
            error_codes(&r),
            src
        );
    }
}

#[test]
fn file_level_typed_const_rejects_mismatched_width_e1002() {
    let cases = [
        // t4: file-level const k i8 initialized to i64 var
        "const k i8 = 3\nfn main() {\n    var y i64 = k\n    fmt.println(y)\n}\n",
        // t8: file-level const k i8 passed to i64 param
        "const k i8 = 3\nfn take(v i64) { fmt.println(v) }\nfn main() {\n    take(k)\n}\n",
        // reassignment of file-level const to mismatched target
        "const k i8 = 3\nfn main() {\n    var y i32 = 0\n    y = k\n    fmt.println(y)\n}\n",
        // binary arithmetic with file-level typed const
        "const k u16 = 5\nfn main() {\n    var a u32 = 1\n    c := a + k\n    fmt.println(c)\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:\n{}", src);
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002, got {:?} for:\n{}",
            error_codes(&r),
            src
        );
    }
}

#[test]
fn named_fn_and_method_value_locals_rejects_mismatched_width_e1002() {
    let cases = [
        // t6: call through local bound to named fn
        "fn get() i8 { return 1 }\nfn main() {\n    f := get\n    var y i64 = f()\n    fmt.println(y)\n}\n",
        // t7: call through local bound to method value
        "struct c { n i8 }\nfn (x c) get() i8 { return x.n }\nfn main() {\n    a := c{n: 1}\n    f := a.get\n    var y i64 = f()\n    fmt.println(y)\n}\n",
        // call through local passed to mismatched param
        "fn get() i8 { return 1 }\nfn take(v i32) { fmt.println(v) }\nfn main() {\n    f := get\n    take(f())\n}\n",
        // call through method value reassigned to mismatched target
        "struct c { n i8 }\nfn (x c) get() i8 { return x.n }\nfn main() {\n    a := c{n: 1}\n    f := a.get\n    var y i32 = 0\n    y = f()\n    fmt.println(y)\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:\n{}", src);
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002, got {:?} for:\n{}",
            error_codes(&r),
            src
        );
    }
}

#[test]
fn matching_and_untyped_controls_accepted() {
    let cases = [
        // Control: valid Go, must stay accepted
        "const k i8 = 3\nfn get() i8 { return 1 }\nfn main() {\n    xs := []int{1}\n    var n int = len(xs)\n    var y i8 = k\n    f := get\n    var z i8 = f()\n    var u i64 = 3\n    fmt.println(n, y, z, u)\n}\n",
        // Method value with matching target width
        "struct c { n i8 }\nfn (x c) get() i8 { return x.n }\nfn main() {\n    a := c{n: 1}\n    f := a.get\n    var y i8 = f()\n    fmt.println(y)\n}\n",
        // Untyped file-level const remains assignable to any integer width
        "const c = 5\nfn main() {\n    var a i32 = c\n    var b i64 = c\n    fmt.println(a, b)\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "unexpected diagnostics for:\n{}\n{:?}",
            src,
            r.diagnostics
        );
        assert_eq!(error_codes(&r).len(), 0);
    }
}
