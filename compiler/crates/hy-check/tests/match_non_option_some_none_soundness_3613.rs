//! match_non_option_some_none_soundness.rs — regression tests for the
//! `[soundness]` Row 13 / T13 / T20 hole in `check_match`: a `match` subject
//! with a static non-`option[T]` type must reject `some`/`none` constructor
//! patterns with `HYB-E5001`. Legal `option[T]` matches (variable, function
//! return, struct field, `some(...)` literal) must keep passing.

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

// ---------------------------------------------------------------------------
// 1. Non-`option[T]` subjects with `some`/`none` arms must REJECT (HYB-E5001)
// ---------------------------------------------------------------------------

#[test]
fn match_int_lit_with_some_none_rejects_e5001_3613() {
    let src = "fn main() {\n    match 5 {\n        some(v) => { _ = v }\n        none => { _ = 1 }\n    }\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        error_codes(&r).contains(&"HYB-E5001".to_string()),
        "expected HYB-E5001, got {:?}",
        r.diagnostics
    );
}

#[test]
fn match_int_var_with_some_none_rejects_e5001_3613() {
    let src = "fn main() {\n    x := 5\n    match x {\n        some(v) => { _ = v }\n        none => { _ = 1 }\n    }\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        error_codes(&r).contains(&"HYB-E5001".to_string()),
        "expected HYB-E5001, got {:?}",
        r.diagnostics
    );
}

#[test]
fn match_string_lit_with_some_none_rejects_e5001_3613() {
    let src = "fn main() {\n    match \"hi\" {\n        some(v) => { _ = v }\n        none => { _ = 1 }\n    }\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        error_codes(&r).contains(&"HYB-E5001".to_string()),
        "expected HYB-E5001, got {:?}",
        r.diagnostics
    );
}

#[test]
fn match_bool_lit_with_some_none_rejects_e5001_3613() {
    let src = "fn main() {\n    match true {\n        some(v) => { _ = v }\n        none => { _ = 1 }\n    }\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        error_codes(&r).contains(&"HYB-E5001".to_string()),
        "expected HYB-E5001, got {:?}",
        r.diagnostics
    );
}

#[test]
fn match_slice_lit_with_some_none_rejects_e5001_3613() {
    let src = "fn main() {\n    match [1, 2] {\n        some(v) => { _ = v }\n        none => { _ = 1 }\n    }\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        error_codes(&r).contains(&"HYB-E5001".to_string()),
        "expected HYB-E5001, got {:?}",
        r.diagnostics
    );
}

#[test]
fn match_float_lit_with_some_none_rejects_e5001_3613() {
    let src = "fn main() {\n    match 3.14 {\n        some(v) => { _ = v }\n        none => { _ = 1 }\n    }\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        error_codes(&r).contains(&"HYB-E5001".to_string()),
        "expected HYB-E5001, got {:?}",
        r.diagnostics
    );
}

#[test]
fn match_struct_with_some_none_rejects_e5001_3613() {
    let src = "struct item {\n    val int\n}\nfn main() {\n    it := item{val: 1}\n    match it {\n        some(v) => { _ = v }\n        none => { _ = 1 }\n    }\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        error_codes(&r).contains(&"HYB-E5001".to_string()),
        "expected HYB-E5001, got {:?}",
        r.diagnostics
    );
}

#[test]
fn match_map_with_some_none_rejects_e5001_3613() {
    let src = "fn main() {\n    m := map[string]int{\"a\": 1}\n    match m {\n        some(v) => { _ = v }\n        none => { _ = 1 }\n    }\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        error_codes(&r).contains(&"HYB-E5001".to_string()),
        "expected HYB-E5001, got {:?}",
        r.diagnostics
    );
}

// ---------------------------------------------------------------------------
// 2. Legal `option[T]` matches must keep ACCEPTING (no regression)
// ---------------------------------------------------------------------------

#[test]
fn match_option_var_some_none_accepted_3613() {
    let src = "fn main() {\n    o := some(2)\n    match o {\n        some(v) => { _ = v }\n        none => { _ = 1 }\n    }\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    assert!(error_codes(&r).is_empty(), "{:?}", r.diagnostics);
}

#[test]
fn match_some_lit_some_none_accepted_3613() {
    let src = "fn main() {\n    match some(7) {\n        some(v) => { _ = v }\n        none => { _ = 1 }\n    }\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    assert!(error_codes(&r).is_empty(), "{:?}", r.diagnostics);
}

#[test]
fn match_option_struct_field_some_none_accepted_3613() {
    let src = "struct box {\n    v option[int]\n}\nfn main() {\n    b := box{v: some(9)}\n    match b.v {\n        some(v) => { _ = v }\n        none => { _ = 1 }\n    }\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    assert!(error_codes(&r).is_empty(), "{:?}", r.diagnostics);
}

#[test]
fn match_option_with_guard_and_wildcard_accepted_3613() {
    let src = "fn main() {\n    o := some(3)\n    match o {\n        some(v) if v > 0 => { _ = v }\n        some(w) => { _ = w }\n        none => { _ = 1 }\n    }\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    assert!(error_codes(&r).is_empty(), "{:?}", r.diagnostics);
}
