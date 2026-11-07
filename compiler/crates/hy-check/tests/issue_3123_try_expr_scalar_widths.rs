//! issue_3123_try_expr_scalar_widths.rs — issue #3123 (Row 13 BUILDABLE).
//!
//! Typed widths from error-propagating '?' expressions escaped the #2712
//! width check (HYB-E1002) because `Expr::Try` was not unwrapped to resolve
//! its first result scalar in `typed_value_scalar`.

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
fn try_expr_scalar_mismatches_reject_e1002() {
    let cases = [
        (
            "Row 1: var decl with mismatched scalar",
            "fn get() (int8, error) { return 1, nil }\nfn main() {\n    var y int64 = get()?\n    _ = y\n}\n",
        ),
        (
            "Row 2: short decl inference propagated to mismatched var",
            "fn get() (int8, error) { return 1, nil }\nfn main() {\n    x := get()?\n    var y int64 = x\n    _ = y\n}\n",
        ),
        (
            "Row 3: assignment to mismatched local",
            "fn get() (int8, error) { return 1, nil }\nfn main() {\n    var y int64 = 0\n    y = get()?\n    _ = y\n}\n",
        ),
        (
            "Row 4: call argument to mismatched param",
            "fn get() (int8, error) { return 1, nil }\nfn take(v int64) { _ = v }\nfn main() {\n    take(get()?)\n}\n",
        ),
        (
            "Row 5: binary arithmetic operation with mixed widths",
            "fn get() (int8, error) { return 1, nil }\nfn main() {\n    var y int64 = 0\n    y = y + get()?\n    _ = y\n}\n",
        ),
        (
            "Row 6: slice literal element mismatch",
            "fn get() (int8, error) { return 1, nil }\nfn main() {\n    xs := []int64{get()?}\n    _ = xs\n}\n",
        ),
        (
            "Row 7: map literal value mismatch",
            "fn get() (int8, error) { return 1, nil }\nfn main() {\n    m := map[string]int64{\"a\": get()?}\n    _ = m\n}\n",
        ),
        (
            "Row 8: map literal key mismatch",
            "fn get() (int8, error) { return 1, nil }\nfn main() {\n    m := map[int64]string{get()?: \"a\"}\n    _ = m\n}\n",
        ),
        (
            "Row 9: struct literal field mismatch",
            "struct item { val int64 }\nfn get() (int8, error) { return 1, nil }\nfn main() {\n    a := item{val: get()?}\n    _ = a\n}\n",
        ),
        (
            "Row 10: append argument mismatch",
            "fn get() (int8, error) { return 1, nil }\nfn main() {\n    xs := []int64{1}\n    xs = append(xs, get()?)\n    _ = xs\n}\n",
        ),
        (
            "Row 11: method call with try mismatch",
            "struct worker {}\nfn (w worker) get() (int8, error) { return 1, nil }\nfn main() {\n    w := worker{}\n    var y int64 = w.get()?\n    _ = y\n}\n",
        ),
    ];

    for (name, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected reject for case: {name}\nsrc:\n{src}"
        );
        let codes = error_codes(&r);
        assert!(
            codes.contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 in {codes:?} for case: {name}"
        );
    }
}

#[test]
fn try_expr_scalar_valid_controls_accept() {
    let src = r#"
struct item { val int8 }
struct worker {}
fn (w worker) get() (int8, error) { return 1, nil }
fn get() (int8, error) { return 1, nil }
fn take(v int8) { _ = v }

fn main() {
    var y int8 = get()?
    x := get()?
    var z int8 = x
    y = get()?
    take(get()?)
    var t int8 = y + get()?
    xs := []int8{get()?}
    m1 := map[string]int8{"a": get()?}
    m2 := map[int8]string{get()?: "a"}
    a := item{val: get()?}
    xs = append(xs, get()?)
    w := worker{}
    var ym int8 = w.get()?
    fmt.println(y, z, t, xs, m1, m2, a, ym)
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept for valid try controls: {:?}",
        r.diagnostics
    );
    let errors = error_codes(&r);
    assert!(
        errors.is_empty(),
        "expected 0 error diagnostics, got: {errors:?}"
    );
}
