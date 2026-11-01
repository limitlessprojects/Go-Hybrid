//! append_nil_2745.rs — issue #2745 (Row 13 BUILDABLE).
//!
//! `append` accepted two unbuildable forms: an untyped `nil` as the FIRST
//! argument (`first argument to append must be a slice; have untyped nil`) and
//! `nil` appended to a slice with a NON-nilable element type (`cannot use nil
//! as T value in argument to append`). `nil` resolves to `OpKind::Unknown`, so
//! both the slice-kind check and `kinds_conflict` failed open.

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
fn nil_first_argument_to_append_rejects() {
    let src = "\
fn main() {
    _ = append(nil, 1)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E2005" || c == "HYB-E1002"),
        "{:?}",
        r.diagnostics
    );
}

#[test]
fn nil_into_non_nilable_slice_rejects_e1002() {
    for (name, src) in [
        (
            "[]int",
            "fn main() {\n    var is []int\n    _ = append(is, nil)\n}\n",
        ),
        (
            "[]string",
            "fn main() {\n    var ss []string\n    _ = append(ss, nil)\n}\n",
        ),
        (
            "[]bool",
            "fn main() {\n    var bs []bool\n    _ = append(bs, nil)\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "{name}");
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "{name}: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn nil_into_nilable_slice_stays_accepted() {
    for (name, src) in [
        (
            "[]*int",
            "fn main() {\n    var ps []*int\n    _ = append(ps, nil)\n}\n",
        ),
        (
            "[]any",
            "fn main() {\n    var anys []any\n    _ = append(anys, nil)\n}\n",
        ),
        (
            "control append(xs, 1)",
            "fn main() {\n    var is []int\n    _ = append(is, 1)\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "{name}: {:?}", r.diagnostics);
        assert!(error_codes(&r).is_empty(), "{name}: {:?}", r.diagnostics);
    }
}
