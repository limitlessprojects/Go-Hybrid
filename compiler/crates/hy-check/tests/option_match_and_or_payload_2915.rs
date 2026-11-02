//! Regression tests for Issue #2915:
//! `option` match payload binding `some(v)` is typed, and `option[T].or(default)`
//! checks argument type against T (Row 13 BUILDABLE, HYB-E1002).

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
fn match_some_binding_type_mismatch_rejected() {
    for (name, src) in [
        (
            "var init mismatch",
            r#"
fn find() option[int] { return some(1) }
fn main() {
    match find() {
        some(v): {
            var s string = v
            _ = s
        }
        none: {}
    }
}
"#,
        ),
        (
            "arithmetic mismatch",
            r#"
fn find() option[int] { return some(1) }
fn main() {
    match find() {
        some(v): {
            _ = v + "a"
        }
        none: {}
    }
}
"#,
        ),
        (
            "fn call argument mismatch",
            r#"
fn find() option[int] { return some(1) }
fn take_str(s string) {}
fn main() {
    match find() {
        some(v): take_str(v)
        none: {}
    }
}
"#,
        ),
        (
            "return value mismatch",
            r#"
fn find() option[int] { return some(1) }
fn get_str() string {
    match find() {
        some(v): return v
        none: return "n"
    }
}
fn main() {}
"#,
        ),
        (
            "nested slice element mismatch",
            r#"
fn find() option[[]int] { return some([]int{1}) }
fn main() {
    match find() {
        some(xs): {
            var s string = xs[0]
            _ = s
        }
        none: {}
    }
}
"#,
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected Reject for {name}");
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for {name}, got {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn option_or_argument_mismatch_rejected() {
    let src = r#"
fn find() option[int] { return some(1) }
fn main() {
    _ = find().or("a")
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for option.or mismatch, got {:?}",
        r.diagnostics
    );
}

#[test]
fn option_match_and_or_valid_stays_accepted() {
    let src = r#"
fn find() option[int] { return some(1) }
fn main() {
    match find() {
        some(v): {
            var x int = v
            _ = x
        }
        none: {}
    }
    _ = find().or(0)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept, got {:?}", r.diagnostics);
    assert!(error_codes(&r).is_empty(), "expected no errors, got {:?}", r.diagnostics);
}
