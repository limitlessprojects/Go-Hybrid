//! Soundness checks for direct closure invocations and closure calls (Row 13 BUILDABLE, #4615).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "test.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn errors(r: &hy_check::CheckResult) -> Vec<(String, String)> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| (d.code.clone(), d.message.clone()))
        .collect()
}

fn assert_e1002(src: &str, what: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected Reject for {what}, got Accept (0 diagnostics)"
    );
    let errs = errors(&r);
    assert!(
        errs.iter().any(|(code, _)| code == "HYB-E1002"),
        "expected HYB-E1002 for {what}, got {errs:?}"
    );
}

#[test]
fn rejects_closure_arity_mismatch() {
    let too_few = r#"
package main
fn main() {
    (fn(a int, b int) {})(42)
}
"#;
    assert_e1002(too_few, "too few arguments to direct closure");

    let too_many = r#"
package main
fn main() {
    (fn(a int) {})(42, 43)
}
"#;
    assert_e1002(too_many, "too many arguments to direct closure");
}

#[test]
fn rejects_closure_type_mismatch() {
    let string_for_int = r#"
package main
fn main() {
    (fn(x int) {})("mismatched_type")
}
"#;
    assert_e1002(string_for_int, "string passed to int closure param");
}

#[test]
fn rejects_closure_scalar_width_mismatch() {
    let int64_for_int32 = r#"
package main
fn main() {
    var y int64 = 42
    (fn(x int32) {})(y)
}
"#;
    assert_e1002(int64_for_int32, "int64 passed to int32 closure param");
}

#[test]
fn rejects_local_closure_type_mismatch() {
    let src = r#"
package main
fn main() {
    f := fn(x int) {}
    f("mismatched_type")
}
"#;
    assert_e1002(src, "string passed to local closure int param");
}

#[test]
fn rejects_local_closure_scalar_width_mismatch() {
    let src = r#"
package main
fn main() {
    f := fn(x int32) {}
    var y int64 = 42
    f(y)
}
"#;
    assert_e1002(src, "int64 passed to local closure int32 param");
}

#[test]
fn rejects_spawn_closure_type_mismatch() {
    let src = r#"
package main
fn main() {
    f := fn(x int) {}
    task f("mismatched_type")
}
"#;
    assert_e1002(src, "string passed to spawn local closure");

    let direct = r#"
package main
fn main() {
    var y int64 = 42
    task (fn(x int32) {})(y)
}
"#;
    assert_e1002(direct, "int64 passed to direct spawn closure");
}

