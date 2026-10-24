//! array_len_nonconst_1133.rs — regressions for issue #1133:
//! array length that is a `len`/`cap` call over a non-constant argument
//! (`[len(ch)]int`, `[cap(ch)-1]int`, `[len(slice)]int`, `[len(str_var)]int`)
//! must be rejected with HYB-E6004 instead of failing at `go build`.

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

fn rejects_with(src: &str, code: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected reject for:\n{src}\ndiags: {:?}",
        r.diagnostics
    );
    assert!(
        error_codes(&r).contains(&code.to_string()),
        "expected error code {code}, got {:?} for:\n{src}",
        error_codes(&r)
    );
}

fn accepts_without(src: &str, code: &str) {
    let r = check(src);
    assert!(
        !error_codes(&r).contains(&code.to_string()),
        "expected no {code}, got {:?} for:\n{src}",
        error_codes(&r)
    );
}

// ---------------------------------------------------------------------------
// Negative controls (issue #1133) — must reject with HYB-E6004
// ---------------------------------------------------------------------------

#[test]
fn len_channel_rejected() {
    let src = "fn test() {\n    ch := make(chan int, 4)\n    var xs [len(ch)]int\n    _ = xs\n}\n";
    rejects_with(src, "HYB-E6004");
}

#[test]
fn cap_channel_rejected() {
    let src = "fn test() {\n    ch := make(chan int, 4)\n    var xs [cap(ch)]int\n    _ = xs\n}\n";
    rejects_with(src, "HYB-E6004");
}

#[test]
fn len_slice_rejected() {
    let src = "fn test() {\n    ys := []int{1, 2}\n    var xs [len(ys)]int\n    _ = xs\n}\n";
    rejects_with(src, "HYB-E6004");
}

#[test]
fn len_string_var_rejected() {
    let src = "fn test() {\n    s := \"hello\"\n    var xs [len(s)]int\n    _ = xs\n}\n";
    rejects_with(src, "HYB-E6004");
}

#[test]
fn cap_channel_arithmetic_rejected() {
    let src =
        "fn test() {\n    ch := make(chan int, 2)\n    var xs [cap(ch) - 1]int\n    _ = xs\n}\n";
    rejects_with(src, "HYB-E6004");
}

#[test]
fn len_map_rejected() {
    let src = "fn test() {\n    m := map[string]int{}\n    var xs [len(m)]int\n    _ = xs\n}\n";
    rejects_with(src, "HYB-E6004");
}

#[test]
fn len_channel_composite_lit_rejected() {
    let src = "fn test() {\n    ch := make(chan int, 4)\n    _ = [len(ch)]int{}\n}\n";
    rejects_with(src, "HYB-E6004");
}

// ---------------------------------------------------------------------------
// Positive controls — must stay accepted without HYB-E6004
// ---------------------------------------------------------------------------

#[test]
fn literal_and_arith_array_len_accepted() {
    let src = "fn test() {\n    var xs [3]int\n    var ys [1 + 2]int\n    _ = xs\n    _ = ys\n}\n";
    accepts_without(src, "HYB-E6004");
}

#[test]
fn len_string_lit_accepted() {
    let src = "fn test() {\n    var xs [len(\"hello\")]int\n    _ = xs\n}\n";
    accepts_without(src, "HYB-E6004");
}

#[test]
fn len_file_const_string_accepted() {
    let src = "const s = \"hello\"\nfn test() {\n    var xs [len(s)]int\n    _ = xs\n}\n";
    accepts_without(src, "HYB-E6004");
}

#[test]
fn len_folded_string_concat_accepted() {
    let src = "fn test() {\n    var xs [len(\"a\" + \"b\")]int\n    _ = xs\n}\n";
    accepts_without(src, "HYB-E6004");
}

#[test]
fn array_literal_with_len_const_string_accepted() {
    let src = "fn test() {\n    _ = [len(\"hello\")]int{}\n}\n";
    accepts_without(src, "HYB-E6004");
}

#[test]
fn len_and_cap_over_array_binding_accepted() {
    let src = "fn test() {\n    var a [4]int\n    var xs [len(a)]int\n    var ys [cap(a)]int\n    _ = xs\n    _ = ys\n}\n";
    accepts_without(src, "HYB-E6004");
}
