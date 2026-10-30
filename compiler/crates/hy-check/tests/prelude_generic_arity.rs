//! prelude_generic_arity.rs — regression tests for #1878: `set` and `option`
//! take exactly one type argument.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn set_with_two_args_is_e1002() {
    let r = check("struct record {\n    tags set[string, int]\n}\n\nfn main() {\n    var r record\n    _ = r\n}\n");
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for `set[string, int]`, got {cs:?}"
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn option_with_three_args_is_already_rejected_by_the_parser() {
    // `option[...]` is parsed by a dedicated grammar rule that accepts one
    // type, so a three-argument form never reaches the checker. Pin that so
    // the issue's second shape cannot regress into a silent accept.
    let err = parse_file(
        "inline.hy",
        "struct record {\n    active option[bool, string, int]\n}\n",
    )
    .expect_err("a three-argument option must not parse");
    let joined = format!("{err:?}");
    assert!(
        joined.contains("HYB-E0002"),
        "expected a parse diagnostic for option[bool, string, int], got {joined}"
    );
}

#[test]
fn single_argument_prelude_generics_accept() {
    let r = check("struct record {\n    tags set[string]\n    active option[bool]\n}\n\nfn main() {\n    var r record\n    _ = r\n}\n");
    let cs = codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E1002"),
        "single-argument set/option must stay accepted, got {cs:?}"
    );
}
