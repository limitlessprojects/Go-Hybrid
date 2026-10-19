//! #720 — Builtin functions `copy`, `panic`, `recover`, `clear` bypass arity
//! and argument-shape checks (HYB-E2005). Invalid invocations pass `hyb check`
//! with 0 diagnostics but emit Go that fails `go build` / `go vet`.
//!
//! Type information is not available at this pass, so only literal arguments
//! can be checked. Variable operands fail open.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {diags:?}"));
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
fn copy_with_wrong_arity_is_rejected() {
    for src in ["fn main() { copy() }", "fn main() { copy(1) }", "fn main() { copy(1, 2, 3) }"] {
        let r = check(src);
        let cs = error_codes(&r);
        assert!(
            cs.iter().any(|c| c == "HYB-E2005"),
            "expected HYB-E2005 for copy with wrong arity ({src:?}), got {cs:?}"
        );
        assert_eq!(r.verdict, Verdict::Reject);
    }
}

#[test]
fn copy_with_literal_destination_is_rejected() {
    // copy(1, s) — destination is an int literal, not a slice.
    let r = check("fn main() { s := []int{1}; copy(1, s) }");
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005 for copy(1, s), got {cs:?}"
    );
    assert_eq!(r.verdict, Verdict::Reject);

    // copy("hello", s) — destination is a string literal, not a slice.
    let r = check("fn main() { s := []int{1}; copy(\"hello\", s) }");
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005 for copy(string_lit, s), got {cs:?}"
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn copy_with_non_slice_or_string_source_is_rejected() {
    // copy(s, 1) — source is an int literal, not a slice or string.
    let r = check("fn main() { s := []int{1}; copy(s, 1) }");
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005 for copy(s, 1), got {cs:?}"
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn copy_with_valid_slice_arguments_accepts() {
    let r = check(
        "fn main() {\n\
         \ts := []int{1}\n\
         \td := []int{}\n\
         \tcopy(d, s)\n\
         }\n",
    );
    assert!(
        r.diagnostics.is_empty(),
        "valid copy(slice, slice) must accept: {:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn copy_with_string_source_accepts() {
    // copy(dst_slice, string) is valid in Go.
    let r = check(
        "fn main() {\n\
         \ts := []byte{}\n\
         \tcopy(s, \"hello\")\n\
         }\n",
    );
    assert!(
        r.diagnostics.is_empty(),
        "valid copy(slice, string) must accept: {:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn panic_with_wrong_arity_is_rejected() {
    for src in ["fn main() { panic() }", "fn main() { panic(1, 2) }"] {
        let r = check(src);
        let cs = error_codes(&r);
        assert!(
            cs.iter().any(|c| c == "HYB-E2005"),
            "expected HYB-E2005 for panic with wrong arity ({src:?}), got {cs:?}"
        );
        assert_eq!(r.verdict, Verdict::Reject);
    }
}

#[test]
fn panic_with_one_argument_accepts() {
    let r = check("fn main() { panic(1) }");
    assert!(
        r.diagnostics.is_empty(),
        "panic(1) must accept: {:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn recover_with_wrong_arity_is_rejected() {
    for src in ["fn main() { recover(1) }", "fn main() { recover(1, 2) }"] {
        let r = check(src);
        let cs = error_codes(&r);
        assert!(
            cs.iter().any(|c| c == "HYB-E2005"),
            "expected HYB-E2005 for recover with wrong arity ({src:?}), got {cs:?}"
        );
        assert_eq!(r.verdict, Verdict::Reject);
    }
}

#[test]
fn recover_with_no_arguments_accepts() {
    let r = check("fn main() { recover() }");
    assert!(
        r.diagnostics.is_empty(),
        "recover() must accept: {:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn clear_with_wrong_arity_is_rejected() {
    for src in ["fn main() { clear() }", "fn main() { clear(1, 2) }"] {
        let r = check(src);
        let cs = error_codes(&r);
        assert!(
            cs.iter().any(|c| c == "HYB-E2005"),
            "expected HYB-E2005 for clear with wrong arity ({src:?}), got {cs:?}"
        );
        assert_eq!(r.verdict, Verdict::Reject);
    }
}

#[test]
fn clear_with_literal_argument_is_rejected() {
    // clear(1) — int literal cannot be cleared.
    let r = check("fn main() { clear(1) }");
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005 for clear(1), got {cs:?}"
    );
    assert_eq!(r.verdict, Verdict::Reject);

    // clear("hello") — string literal cannot be cleared.
    let r = check("fn main() { clear(\"hello\") }");
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005 for clear(string literal), got {cs:?}"
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn clear_with_slice_or_map_accepts() {
    let r = check(
        "fn main() {\n\
         \ts := []int{1}\n\
         \tm := map[string]int{}\n\
         \tclear(s)\n\
         \tclear(m)\n\
         }\n",
    );
    assert!(
        r.diagnostics.is_empty(),
        "clear(slice) and clear(map) must accept: {:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Accept);
}
