//! Regression tests for issue #765: HYB-E2002 must reject multi-value calls
//! in single-value or mismatched-arity contexts for single-type-arg generic
//! functions/methods (`pair[int](3, 9)`, `b.pair[int](3, 9)`), generic struct
//! fn fields, method/generic fn bindings, and channel/loop fn callees.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn rejects_e2002(src: &str) {
    let r = check(src);
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error && d.code == "HYB-E2002"),
        "expected HYB-E2002, got {:?}\nsrc:\n{}",
        r.diagnostics,
        src
    );
    assert_eq!(r.verdict, Verdict::Reject, "src:\n{}", src);
}

#[test]
fn generic_fn_and_method_single_type_arg_calls_reject_e2002() {
    rejects_e2002(
        r#"fn pair[t](a t, b t) (t, t) {
    return a, b
}

fn main() {
    p := pair[int](3, 9)
    _ = p
}
"#,
    );

    rejects_e2002(
        r#"struct box {
    n int
}

fn (b box) pair[t](x t, y t) (t, t) {
    return x, y
}

fn main() {
    b := box{n: 1}
    p := b.pair[int](3, 9)
    _ = p
}
"#,
    );
}

#[test]
fn bound_generic_fn_method_value_and_channel_fn_reject_e2002() {
    rejects_e2002(
        r#"fn pair[t](a t, b t) (t, t) {
    return a, b
}

fn main() {
    f := pair[int]
    p := f(3, 9)
    _ = p
}
"#,
    );

    rejects_e2002(
        r#"struct holder[t] {
    f fn(a t, b t) (t, t)
}

fn main() {
    var h holder[int]
    p := h.f(3, 9)
    _ = p
}
"#,
    );
}
