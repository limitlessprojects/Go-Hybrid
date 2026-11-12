//! Regression test for Issue #4223: associated functions skip member and argument resolution
//! b.new(2) and box.new("s") must reject with HYB-E1002 under Row 13 (BUILDABLE) and SPEC-LANGUAGE LX11.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).expect("test snippet must parse");
    check_file(&ast)
}

fn assert_rejects_e1002(src: &str) {
    let result = check(src);
    assert_eq!(result.verdict, Verdict::Reject, "src:\n{src}");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error && d.code == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        result.diagnostics
    );
}

fn assert_accepts(src: &str) {
    let result = check(src);
    assert_eq!(
        result.verdict,
        Verdict::Accept,
        "control must stay accepted: {:?}",
        result.diagnostics
    );
}

const BOX_DEF: &str = r#"struct box { v int }

fn box.new(v int) box {
    return box{v: v}
}
"#;

#[test]
fn f1_calling_associated_fn_on_instance_rejects() {
    let src = format!(
        r#"{BOX_DEF}
fn main() {{
    b := box.new(1)
    c := b.new(2)
    fmt.println(c.v)
}}
"#
    );
    assert_rejects_e1002(&src);
}

#[test]
fn f2_associated_fn_call_with_mismatched_arg_type_rejects() {
    let src = format!(
        r#"{BOX_DEF}
fn main() {{
    b := box.new("s")
    fmt.println(b.v)
}}
"#
    );
    assert_rejects_e1002(&src);
}

#[test]
fn control_associated_fn_call_valid_args_accepts() {
    let src = format!(
        r#"{BOX_DEF}
fn main() {{
    b := box.new(1)
    fmt.println(b.v)
}}
"#
    );
    assert_accepts(&src);
}

#[test]
fn control_non_existent_method_on_instance_rejects() {
    let src = format!(
        r#"{BOX_DEF}
fn main() {{
    b := box.new(1)
    b.nope()
}}
"#
    );
    assert_rejects_e1002(&src);
}
