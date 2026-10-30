//! assignment_target_resolution_1574.rs — regression for issue #1574:
//! [buildable] assignment targets are never resolved: `f = 2` on a function declaration,
//! `fmt = 1`, `len = 1` and `holder = 1` pass hyb check, fail go build.
//!
//! An assignment target must resolve to a mutable variable/place, not a function
//! declaration, imported package root, built-in function, or type name.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn error_messages(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.message.clone())
        .collect()
}

// ---------------------------------------------------------------------------
// Issue #1574 repro shapes
// ---------------------------------------------------------------------------

#[test]
fn assign_to_fn_declaration_rejects_e1002() {
    let r = check("fn f() int {\n  return 1\n}\nfn main() {\n  f = 2\n  fmt.println(f)\n}");
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(error_messages(&r)
        .iter()
        .any(|m| m.contains("cannot assign to f")));
}

#[test]
fn inc_on_fn_declaration_rejects_e1002() {
    let r = check("fn f() int {\n  return 1\n}\nfn main() {\n  f++\n  fmt.println(f)\n}");
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(error_messages(&r)
        .iter()
        .any(|m| m.contains("cannot assign to f")));
}

#[test]
fn add_assign_on_fn_declaration_rejects_e1002() {
    let r = check("fn f() int {\n  return 1\n}\nfn main() {\n  f += 1\n  fmt.println(f)\n}");
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(error_messages(&r)
        .iter()
        .any(|m| m.contains("cannot assign to f")));
}

#[test]
fn multi_target_assign_to_fn_declaration_rejects_e1002() {
    let r = check("fn f() int {\n  return 1\n}\nfn main() {\n  var x int\n  f, x = 1, 2\n  fmt.println(x)\n}");
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(error_messages(&r)
        .iter()
        .any(|m| m.contains("cannot assign to f")));
}

#[test]
fn assign_to_imported_package_rejects_e1002() {
    let r = check("import \"go/fmt\"\nfn main() {\n  fmt = 1\n}");
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(error_messages(&r)
        .iter()
        .any(|m| m.contains("use of package fmt not in selector")));
}

#[test]
fn assign_to_predeclared_builtin_rejects_e1002() {
    let r = check("fn main() {\n  len = 1\n}");
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(error_messages(&r)
        .iter()
        .any(|m| m.contains("len (built-in) must be called")));
}

#[test]
fn assign_to_local_struct_type_rejects_e1002() {
    let r = check("struct holder {\n  n int\n}\nfn main() {\n  holder = 1\n}");
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(error_messages(&r)
        .iter()
        .any(|m| m.contains("holder (type) is not an expression")));
}

#[test]
fn assign_to_predeclared_type_rejects_e1002() {
    let r = check("fn main() {\n  error = 1\n}");
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(error_messages(&r)
        .iter()
        .any(|m| m.contains("error (type) is not an expression")));
}

// ---------------------------------------------------------------------------
// Positive controls that must stay accepted
// ---------------------------------------------------------------------------

#[test]
fn positive_controls_local_variable_assignment_and_inc() {
    let r = check(
        "fn main() {\n\
             var x int\n\
             x = 2\n\
             x++\n\
             x += 1\n\
             fmt.println(x)\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn positive_controls_shadowing_builtin_name() {
    let r = check(
        "fn main() {\n\
             len := 5\n\
             len = 6\n\
             fmt.println(len)\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn positive_controls_shadowing_package_name() {
    let r = check(
        "fn main() {\n\
             fmt := 1\n\
             fmt = 2\n\
             _ = fmt\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn positive_controls_wildcard_target() {
    let r = check(
        "fn main() {\n\
             _ = 1\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn positive_controls_fn_in_value_position() {
    let r = check(
        "fn f() int {\n\
             return 1\n\
         }\n\
         fn main() {\n\
             v := f\n\
             fmt.println(v())\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn positive_controls_parameter_and_result_rebinding() {
    let r = check(
        "fn g(x int) int {\n\
             x = 2\n\
             return x\n\
         }\n\
         fn main() {\n\
             fmt.println(g(1))\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn positive_controls_field_and_index_assignment() {
    let r = check(
        "struct s {\n\
             a int\n\
         }\n\
         fn main() {\n\
             var p s\n\
             p.a = 1\n\
             xs := []int{0}\n\
             xs[0] = 1\n\
             m := map[string]int{}\n\
             m[\"k\"] = 1\n\
             fmt.println(p.a, xs[0], m[\"k\"])\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}
