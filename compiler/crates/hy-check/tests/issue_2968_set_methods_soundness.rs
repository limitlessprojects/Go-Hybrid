//! issue_2968_set_methods_soundness.rs — regression tests for set[T] method validation
//! under Row 13 (BUILDABLE) and HYB-E1002 (issue #2968).

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
fn set_non_existent_methods_rejected() {
    let cases = [
        ("s.add(1)", r#"
fn main() {
    var s set[int]
    s.add(1)
}
"#),
        ("s.pop()", r#"
fn main() {
    var s set[int]
    s.pop()
}
"#),
        ("s.clear()", r#"
fn main() {
    var s set[int]
    s.clear()
}
"#),
        ("s.foo(1, 2, 3)", r#"
fn main() {
    var s set[int]
    s.foo(1, 2, 3)
}
"#),
    ];

    for (desc, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected rejection for non-existent method call: {}",
            desc
        );
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for {}, got: {:?}",
            desc,
            error_codes(&r)
        );
    }
}

#[test]
fn set_method_arity_mismatches_rejected() {
    let cases = [
        ("s.insert() with 0 args", r#"
fn main() {
    var s set[int]
    s.insert()
}
"#),
        ("s.insert(1, 2) with 2 args", r#"
fn main() {
    var s set[int]
    s.insert(1, 2)
}
"#),
        ("s.contains() with 0 args", r#"
fn main() {
    var s set[int]
    s.contains()
}
"#),
        ("s.contains(1, 2) with 2 args", r#"
fn main() {
    var s set[int]
    s.contains(1, 2)
}
"#),
        ("s.remove() with 0 args", r#"
fn main() {
    var s set[int]
    s.remove()
}
"#),
        ("s.remove(1, 2) with 2 args", r#"
fn main() {
    var s set[int]
    s.remove(1, 2)
}
"#),
        ("s.len(42) with 1 arg", r#"
fn main() {
    var s set[int]
    s.len(42)
}
"#),
    ];

    for (desc, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected rejection for arity mismatch: {}",
            desc
        );
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for {}, got: {:?}",
            desc,
            error_codes(&r)
        );
    }
}

#[test]
fn set_method_arg_type_mismatches_rejected() {
    let cases = [
        ("s.insert(string) on set[int]", r#"
fn main() {
    var s set[int]
    s.insert("hello")
}
"#),
        ("s.contains(string) on set[int]", r#"
fn main() {
    var s set[int]
    s.contains("hello")
}
"#),
        ("s.remove(string) on set[int]", r#"
fn main() {
    var s set[int]
    s.remove("hello")
}
"#),
    ];

    for (desc, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected rejection for argument type mismatch: {}",
            desc
        );
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for {}, got: {:?}",
            desc,
            error_codes(&r)
        );
    }
}

#[test]
fn valid_set_method_calls_accepted() {
    let src = r#"
fn main() {
    var s set[int]
    s.insert(1)
    b := s.contains(1)
    _ = b
    s.remove(1)
    n := s.len()
    _ = n
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "valid set calls must accept, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn set_method_result_kinds_checked() {
    let cases = [
        ("s.remove in value position", r#"
fn main() {
    var s set[int]
    x := s.remove(1)
    _ = x
}
"#),
        ("s.contains assigned to string", r#"
fn main() {
    var s set[int]
    var str string = s.contains(1)
    _ = str
}
"#),
        ("s.len assigned to string", r#"
fn main() {
    var s set[int]
    var str string = s.len()
    _ = str
}
"#),
    ];

    for (desc, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected rejection for result kind violation: {}",
            desc
        );
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for {}, got: {:?}",
            desc,
            error_codes(&r)
        );
    }
}
