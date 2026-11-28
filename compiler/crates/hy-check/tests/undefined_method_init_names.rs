//! A method is reachable only through its receiver, and `func init` is never
//! a declared package-level name in Go. Using either as a bare identifier
//! (`m()`, `init()`, `f := init`) used to pass `hyb check` through the
//! file-level name table and then fail the emitted Go with `undefined: m` /
//! `undefined: init`. These now report HYB-E1001; legitimate uses stay clean.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn error_codes(src: &str) -> (Vec<String>, Verdict) {
    let ast = parse_file("inline.hy", src).expect("must parse");
    let r = check_file(&ast);
    let codes = r
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect();
    (codes, r.verdict)
}

fn rejects(src: &str) {
    let (codes, verdict) = error_codes(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E1001"),
        "expected HYB-E1001 for {src:?}, got {codes:?}"
    );
    assert_eq!(verdict, Verdict::Reject, "verdict for {src:?}");
}

#[test]
fn bare_method_name_is_unresolved() {
    rejects("struct s { a int }\nfn (x s) m() {}\nfn main() { m() }\n");
}

#[test]
fn bare_init_call_is_unresolved() {
    rejects("fn init() {}\nfn main() { init() }\n");
}

#[test]
fn init_as_value_is_unresolved() {
    rejects("fn init() {}\nfn main() {\n    f := init\n    f()\n}\n");
}

#[test]
fn method_call_through_receiver_and_plain_fn_still_resolve() {
    let (codes, _) = error_codes(
        "struct s { a int }\nfn (x s) m() {}\nfn helper() {}\nfn main() {\n    v := s{a: 1}\n    v.m()\n    helper()\n}\n",
    );
    assert!(
        !codes.iter().any(|c| c == "HYB-E1001"),
        "unexpected HYB-E1001: {codes:?}"
    );
}

#[test]
fn plain_fn_named_like_a_method_still_resolves() {
    let (codes, _) =
        error_codes("struct s { a int }\nfn (x s) m() {}\nfn m() {}\nfn main() { m() }\n");
    assert!(
        !codes.iter().any(|c| c == "HYB-E1001"),
        "unexpected HYB-E1001: {codes:?}"
    );
}
