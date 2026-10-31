use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("snippet parses");
    check_file(&ast)
}

#[test]
fn short_define_type_mutation_in_same_scope_rejected_with_e1002() {
    let out = check(
        r#"
fn main() {
  x := 1
  x, y := "hello", 2
  _ = x
  _ = y
}
"#,
    );
    assert_eq!(out.verdict, Verdict::Reject, "diags: {:?}", out.diagnostics);
    assert!(
        out.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "expected HYB-E1002, got: {:?}",
        out.diagnostics
    );
}

#[test]
fn same_scope_var_redeclaration_rejected_with_e1002() {
    let out = check(
        r#"
fn main() {
  var a int = 1
  var a int = 2
  _ = a
}
"#,
    );
    assert_eq!(out.verdict, Verdict::Reject, "diags: {:?}", out.diagnostics);
    assert!(
        out.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "expected HYB-E1002, got: {:?}",
        out.diagnostics
    );
}

#[test]
fn parameter_clobbered_by_var_in_outer_block_rejected_with_e1002() {
    let out = check(
        r#"
fn foo(x int) {
  var x int = 1
  _ = x
}
fn main() {
  foo(0)
}
"#,
    );
    assert_eq!(out.verdict, Verdict::Reject, "diags: {:?}", out.diagnostics);
    assert!(
        out.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "expected HYB-E1002, got: {:?}",
        out.diagnostics
    );
}

#[test]
fn same_scope_short_define_matching_type_and_inner_block_shadow_accepted() {
    let out = check(
        r#"
fn main() {
  x := 1
  x, y := 2, 3
  if x > 0 {
    var x string = "inner"
    _ = x
  }
  _ = x
  _ = y
}
"#,
    );
    assert_eq!(out.verdict, Verdict::Accept, "diags: {:?}", out.diagnostics);
}
