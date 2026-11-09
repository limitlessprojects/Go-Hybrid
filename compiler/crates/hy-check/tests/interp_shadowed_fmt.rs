//! Regression tests for #3568 (GUARANTEES Row 13 BUILDABLE):
//! string interpolation with an expression part lowers to `fmt.Sprintf(...)`
//! and must reject with `HYB-E1002` when an in-scope binding or package-level
//! declaration named `fmt` captures or collides with the `fmt` package.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn e1002(r: &CheckResult) -> usize {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error && d.code == "HYB-E1002")
        .count()
}

#[test]
fn issue_3568_repro_parameter_and_local_reject_e1002() {
    let r_param = check(
        "package main\n\nimport \"fmt\"\n\nfn show(fmt i32) string {\n    return \"v={fmt}\"\n}\nfn main() {\n    fmt.println(show(3))\n}\n",
    );
    assert_eq!(e1002(&r_param), 1, "expected 1 HYB-E1002: {:?}", r_param.diagnostics);
    assert_eq!(r_param.diagnostics.len(), 1, "unexpected extra diags: {:?}", r_param.diagnostics);
    assert_eq!(r_param.verdict, Verdict::Reject);

    let r_local = check(
        "package main\n\nimport \"os\"\n\nfn main() {\n    fmt := 3\n    s := \"v={fmt}\"\n    os.exit(len(s))\n}\n",
    );
    assert_eq!(e1002(&r_local), 1, "expected 1 HYB-E1002: {:?}", r_local.diagnostics);
    assert_eq!(r_local.diagnostics.len(), 1, "unexpected extra diags: {:?}", r_local.diagnostics);
    assert_eq!(r_local.verdict, Verdict::Reject);
}

#[test]
fn shadowed_fmt_across_fn_closure_control_generic_and_pkg_scopes_rejects_e1002() {
    for (label, src) in [
        (
            "method receiver named fmt",
            "struct box { n int }\nfn (fmt box) show() string {\n    return \"v={fmt.n}\"\n}\nfn main() {\n    b := box{n: 1}\n    _ = b.show()\n}\n",
        ),
        (
            "named result named fmt",
            "fn show(x int) (fmt string) {\n    fmt = \"v={x}\"\n    return\n}\nfn main() {\n    _ = show(1)\n}\n",
        ),
        (
            "closure parameter named fmt",
            "fn main() {\n    f := fn(fmt int) string {\n        return \"v={fmt}\"\n    }\n    _ = f(2)\n}\n",
        ),
        (
            "if-init binding named fmt",
            "fn main() {\n    if fmt := 3; fmt > 0 {\n        s := \"v={fmt}\"\n        _ = s\n    }\n}\n",
        ),
        (
            "for-range binding named fmt",
            "fn main() {\n    for fmt in 0..2 {\n        s := \"v={fmt}\"\n        _ = s\n    }\n}\n",
        ),
        (
            "for-range upper bound interpolation when loop var is fmt",
            "fn main() {\n    m := map[string]int{\"v=1\": 2}\n    x := 1\n    for fmt in 0..m[\"v={x}\"] {\n        _ = fmt\n    }\n}\n",
        ),
        (
            "for-iterate binding named fmt",
            "fn main() {\n    xs := []int{1, 2}\n    for fmt in xs {\n        s := \"v={fmt}\"\n        _ = s\n    }\n}\n",
        ),
        (
            "match payload binding named fmt",
            "fn main() {\n    var opt option[int] = some(5)\n    match opt {\n        some(fmt): {\n            s := \"v={fmt}\"\n            _ = s\n        }\n        none: {}\n    }\n}\n",
        ),
        (
            "select recv binding named fmt",
            "fn main() {\n    ch := make(chan int, 1)\n    ch <- 7\n    select {\n        fmt := <-ch: {\n            s := \"v={fmt}\"\n            _ = s\n        }\n    }\n}\n",
        ),
        (
            "generic fn type parameter named fmt",
            "fn show[fmt any](x fmt) string {\n    return \"v={x}\"\n}\nfn main() {\n    _ = show[int](3)\n}\n",
        ),
        (
            "generic struct method receiver type parameter named fmt",
            "struct wrap[t] { v t }\nfn (w wrap[fmt]) show() string {\n    return \"v={w.v}\"\n}\nfn main() {\n    w := wrap[int]{v: 4}\n    _ = w.show()\n}\n",
        ),
        (
            "generic struct pointer receiver type parameter named fmt",
            "struct wrap[t] { v t }\nfn (w *wrap[fmt]) show() string {\n    return \"v={w.v}\"\n}\nfn main() {\n    w := wrap[int]{v: 4}\n    _ = w.show()\n}\n",
        ),
        (
            "generic fn signature array length with fmt type parameter",
            "fn take[fmt any](x fmt, a [len(\"v={1}\")]int) {\n    _ = x\n    _ = a\n}\nfn main() {}\n",
        ),
        (
            "generic struct field array length with fmt type parameter",
            "struct holder[fmt any] {\n    v fmt\n    a [len(\"v={1}\")]int\n}\nfn main() {}\n",
        ),
        (
            "package-level fn named fmt",
            "fn fmt() int { return 9 }\nfn main() {\n    s := \"v={fmt()}\"\n    _ = s\n}\n",
        ),
        (
            "package-level const named fmt",
            "const fmt = 9\nfn main() {\n    s := \"v={fmt}\"\n    _ = s\n}\n",
        ),
        (
            "package-level struct named fmt",
            "struct fmt { x int }\nfn main() {\n    v := fmt{x: 9}\n    s := \"v={v.x}\"\n    _ = s\n}\n",
        ),
        (
            "package-level type alias named fmt",
            "type fmt = int\nfn main() {\n    var v fmt = 9\n    s := \"v={v}\"\n    _ = s\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(e1002(&r), 1, "{label} must reject with 1 HYB-E1002: {:?}", r.diagnostics);
        assert_eq!(r.verdict, Verdict::Reject, "{label} must reject");
    }
}

#[test]
fn unshadowed_or_non_interpolated_fmt_usages_stay_accepted() {
    for (label, src) in [
        (
            "plain string literal with fmt param in scope",
            "fn show(fmt i32) string {\n    _ = fmt\n    return \"plain literal\"\n}\nfn main() {\n    _ = show(1)\n}\n",
        ),
        (
            "sibling inner block shadow does not leak to outer interpolation",
            "fn main() {\n    {\n        fmt := 3\n        _ = fmt\n    }\n    x := 4\n    s := \"v={x}\"\n    _ = s\n}\n",
        ),
        (
            "same-statement short var decl evaluates RHS before binding fmt",
            "fn main() {\n    x := 5\n    fmt := \"v={x}\"\n    _ = fmt\n}\n",
        ),
        (
            "struct field and method named fmt do not shadow package fmt",
            "struct item { fmt int }\nfn (it item) fmt_val() int { return it.fmt }\nfn main() {\n    it := item{fmt: 8}\n    s := \"v={it.fmt}:{it.fmt_val()}\"\n    _ = s\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(e1002(&r), 0, "{label} must not emit HYB-E1002: {:?}", r.diagnostics);
        assert_eq!(r.verdict, Verdict::Accept, "{label} must accept: {:?}", r.diagnostics);
    }
}
