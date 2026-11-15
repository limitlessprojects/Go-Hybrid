//! issue_4068_type_args_and_match_patterns.rs — regression for issue #4068:
//! two HYB-E1001 escape hatches that passed `hyb check` with 0 diagnostics
//! and then failed the emitted Go with `undefined`:
//!
//! 1. Type-position bracket arguments: `set[undefined_ty]{}` resolved the
//!    composite literal through `run_type`, which passed
//!    `check_named_types = false` to `run_bracketed_type_args` — so any name
//!    inside `[...]` of a type annotation/composite-literal type was never
//!    checked, while the same name in value position (`x := set[undefined_ty]{}`)
//!    was. Now type-position bracket args are checked exactly like
//!    value-position ones.
//!
//! 2. Dotted match patterns: `MatchPattern::Path` with 2+ segments
//!    (`color.undefined_variant`) was skipped entirely (`segments.len() == 1`
//!    guard), so a match on an enum qualifier that nothing declares passed.
//!    The qualifier head is now routed through `check_type_name`, which keeps
//!    the resolver's fail-open shape: package-qualified roots
//!    (`io.reader_kind`) and declared-enum qualifiers (`color.red`) stay
//!    accepted; only a head that nothing declares is rejected.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects_with_e1001(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1001"),
        "expected HYB-E1001 for {:?}, got {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts_without_e1001(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E1001"),
        "must NOT fire HYB-E1001 for {:?}, got {:?}",
        src,
        cs
    );
}

// ------------------------------------------------------- reject (issue #4068)

#[test]
fn set_literal_undefined_type_arg_rejects() {
    // The issue's literal repro: `set[undefined_ty]{}` passed hy-check.
    rejects_with_e1001("fn main() {\n    s := set[undefined_ty]{}\n    _ = s\n}\n");
}

#[test]
fn var_annotation_undefined_type_arg_rejects() {
    rejects_with_e1001("fn main() {\n    var m set[undefined_ty2]\n    _ = m\n}\n");
}

#[test]
fn struct_field_undefined_type_arg_rejects() {
    rejects_with_e1001(
        "struct holder {\n    items set[undefined_ty3]\n}\nfn main() {\n    _ = 1\n}\n",
    );
}

#[test]
fn match_pattern_undefined_enum_qualifier_rejects() {
    // Two-segment pattern whose qualifier head nothing declares.
    rejects_with_e1001(
        "fn main() {\n    match c {\n        color.undefined_variant => {}\n        _ => {}\n    }\n}\n",
    );
}

#[test]
fn match_pattern_three_segment_undefined_head_rejects() {
    rejects_with_e1001(
        "fn main() {\n    match c {\n        pkg.shape.circle => {}\n        _ => {}\n    }\n}\n",
    );
}

#[test]
fn match_pattern_undefined_head_with_binding_rejects() {
    rejects_with_e1001(
        "fn main() {\n    match opt {\n        color.some(v) => { _ = v }\n        _ => {}\n    }\n}\n",
    );
}

// ------------------------------------------------- accept (no false positives)

#[test]
fn set_literal_predeclared_type_arg_accepts() {
    accepts_without_e1001("fn main() {\n    s := set[int]{}\n    _ = s\n}\n");
}

#[test]
fn var_annotation_local_type_arg_accepts() {
    accepts_without_e1001(
        "struct box[t any] { v t }\nfn main() {\n    var b box[int]\n    _ = b\n}\n",
    );
}

#[test]
fn fn_type_param_bracket_args_accept() {
    // Regression guard for the `check_named_types` flip: fn-generic bracket
    // args (ast_helpers_soundness_3019 shape) must stay clean.
    accepts_without_e1001(
        "struct box[t any] { cb t }\nfn take(b box[fn(c chan<- int)]) { _ = b }\nfn main() { var b box[fn(c <-chan int)]\n take(b) }\n",
    );
}

#[test]
fn match_pattern_declared_enum_qualifier_accepts() {
    accepts_without_e1001(
        "enum color { red, green }\nfn main() {\n    c := color.red\n    match c {\n        color.red => {}\n        color.green => {}\n        _ => {}\n    }\n}\n",
    );
}

#[test]
fn match_pattern_bare_variant_accepts() {
    // Variant names are file-wide values (see collect_file_tables).
    accepts_without_e1001(
        "enum color { red, green }\nfn main() {\n    c := color.red\n    match c {\n        red => {}\n        green => {}\n        _ => {}\n    }\n}\n",
    );
}
