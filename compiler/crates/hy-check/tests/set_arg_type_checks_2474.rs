//! set_arg_type_checks_2474.rs — regression tests for issue #2474:
//! [soundness] set[T] method calls (.insert, .contains) and delete(s, v)
//! elide argument type checking, failing go build (SPEC §2.3 T9 / Row 13 BUILDABLE).

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src).unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn set_insert_with_mismatched_type_rejected_e1002() {
    let src = r#"
fn main() {
    s := set[int]{}
    bad := "incompatible_string"
    _ = s.insert(bad)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, hy_check::Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for s.insert(string) on set[int], got {:?}",
        codes
    );
}

#[test]
fn set_contains_with_mismatched_type_rejected_e1002() {
    let src = r#"
fn main() {
    s := set[int]{}
    bad := "incompatible_string"
    _ = s.contains(bad)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, hy_check::Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for s.contains(string) on set[int], got {:?}",
        codes
    );
}

#[test]
fn set_delete_with_mismatched_type_rejected_e2005() {
    let src = r#"
fn main() {
    s := set[int]{}
    bad := "incompatible_string"
    delete(s, bad)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, hy_check::Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.contains(&"HYB-E2005".to_string()),
        "expected HYB-E2005 for delete(s, string) on set[int], got {:?}",
        codes
    );
}

#[test]
fn set_delete_with_float_on_int_set_rejected_e2005() {
    let src = r#"
fn main() {
    s := set[int]{}
    bad := 3.14
    delete(s, bad)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, hy_check::Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.contains(&"HYB-E2005".to_string()),
        "expected HYB-E2005 for delete(s, float) on set[int], got {:?}",
        codes
    );
}

#[test]
fn set_method_arity_mismatch_rejected_e1002() {
    let src = r#"
fn main() {
    s := set[int]{}
    _ = s.insert()
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, hy_check::Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for 0-arg insert on set[int], got {:?}",
        codes
    );

    let src2 = r#"
fn main() {
    s := set[int]{}
    _ = s.contains(1, 2)
}
"#;
    let r2 = check(src2);
    assert_eq!(r2.verdict, hy_check::Verdict::Reject);
    let codes2 = error_codes(&r2);
    assert!(
        codes2.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for 2-arg contains on set[int], got {:?}",
        codes2
    );
}

#[test]
fn set_unknown_method_call_rejected_e1002() {
    let src = r#"
fn main() {
    s := set[int]{}
    s.nonexistent(1)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, hy_check::Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for non-existent method call on set[int], got {:?}",
        codes
    );
}

#[test]
fn set_valid_operations_accepted() {
    let src = r#"
fn main() {
    s := set[int]{}
    _ = s.insert(42)
    _ = s.contains(42)
    fmt.println(len(s))
    delete(s, 42)

    str_set := set[string]{}
    _ = str_set.insert("hello")
    _ = str_set.contains("hello")
    fmt.println(len(str_set))
    delete(str_set, "hello")
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        hy_check::Verdict::Accept,
        "valid set operations must be accepted, got diagnostics: {:?}",
        r.diagnostics
    );
}
