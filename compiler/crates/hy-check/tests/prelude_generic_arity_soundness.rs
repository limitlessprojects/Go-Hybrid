//! prelude_generic_arity_soundness.rs — tests for HYB-E1002 validation of
//! type argument arities on prelude generic types `set` and `option`.

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
fn set_with_two_type_arguments_rejects_e1002() {
    let src = "struct record {\n    tags set[string, int]\n}\nfn main() {\n    var r record\n    _ = r\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(r.diagnostics.iter().any(|d| d.message.contains("wrong number of type arguments for prelude generic type `set`")));
}

#[test]
fn set_with_three_type_arguments_rejects_e1002() {
    let src = "struct record {\n    tags set[string, int, bool]\n}\nfn main() {\n    var r record\n    _ = r\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(r.diagnostics.iter().any(|d| d.message.contains("wrong number of type arguments for prelude generic type `set`")));
}

#[test]
fn set_without_type_arguments_rejects_e1002() {
    let src = "fn main() {\n    var s set\n    _ = s\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(r.diagnostics.iter().any(|d| d.message.contains("cannot use generic type `set` without type arguments")));
}

#[test]
fn option_without_type_arguments_rejects_e1002() {
    let src = "fn main() {\n    var o option\n    _ = o\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(r.diagnostics.iter().any(|d| d.message.contains("cannot use generic type `option` without type arguments")));
}

#[test]
fn valid_set_and_option_instantiations_accept() {
    let src = "struct record {\n    tags set[string]\n    active option[bool]\n    nested set[option[int]]\n}\nfn main() {\n    var r record\n    var s set[int]\n    var o option[string]\n    _ = r\n    _ = s\n    _ = o\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected accept, got diagnostics: {:?}", r.diagnostics);
}

#[test]
fn set_with_uninstantiated_option_rejects_e1002() {
    let src = "struct record {\n    items set[option]\n}\nfn main() {\n    var r record\n    _ = r\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(r.diagnostics.iter().any(|d| d.message.contains("cannot use generic type `option` without type arguments")));
}

#[test]
fn set_with_uninstantiated_set_rejects_e1002() {
    let src = "struct record {\n    items set[set]\n}\nfn main() {\n    var r record\n    _ = r\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(r.diagnostics.iter().any(|d| d.message.contains("cannot use generic type `set` without type arguments")));
}
