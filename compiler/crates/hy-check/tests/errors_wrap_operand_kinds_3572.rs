//! #3572: errors.wrap operand kind validation.
use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("parses");
    check_file(&ast)
}

fn has_e1002(r: &hy_check::CheckResult) -> bool {
    r.diagnostics.iter().any(|d| d.code == "HYB-E1002")
}

#[test]
fn wrap_int_first_arg_rejects() {
    let r = check("fn main() {\n    a := errors.wrap(5, \"ctx\")\n    _ = a\n}\n");
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(has_e1002(&r), "{:?}", r.diagnostics);
}

#[test]
fn wrap_string_first_arg_rejects() {
    let r = check("fn main() {\n    b := errors.wrap(\"not an error\", \"ctx\")\n    _ = b\n}\n");
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(has_e1002(&r), "{:?}", r.diagnostics);
}

#[test]
fn wrap_int_second_arg_rejects() {
    let r = check("fn main() {\n    base := errors.new(\"disk full\")\n    c := errors.wrap(base, 7)\n    _ = c\n}\n");
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(has_e1002(&r), "{:?}", r.diagnostics);
}

#[test]
fn wrap_correct_args_accepts() {
    let r = check("fn main() {\n    base := errors.new(\"disk full\")\n    c := errors.wrap(base, \"context\")\n    _ = c\n}\n");
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}
