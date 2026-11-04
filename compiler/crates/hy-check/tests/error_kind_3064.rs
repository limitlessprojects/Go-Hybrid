//! #3064 - the predeclared `error` type had no `OpKind`.
//!
//! `error` fell through `kind_of_named` to `Unknown`, and `kinds_conflict` is
//! fail-open on `Unknown`, so every operand check involving an `error` value
//! passed. Each rejected case below is accepted by `hyb check` on main and
//! fails `go build`: a Row 13 BUILDABLE soundness hole.
//!
//! Reproduced as real `go build` failures first, on the pinned toolchain.
//!
//! In-process harness like the sibling suites: no CLI spawn and no shared temp
//! dir, so it is immune to an external CARGO_TARGET_DIR and to thread races.

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn error_codes(src: &str) -> Vec<String> {
    let ast = parse_file("inline.hy", src).unwrap_or_else(|d| panic!("parse failed: {:?}", d));
    check_file(&ast)
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code)
        .collect()
}

fn rejects(src: &str) {
    let codes = error_codes(src);
    assert!(codes.iter().any(|c| c == "HYB-E1002"), "expected HYB-E1002, got {codes:?}");
}

fn accepts(src: &str) {
    let codes = error_codes(src);
    assert!(codes.is_empty(), "rejected with {codes:?}");
}

#[test]
fn error_argument_into_int_param_is_rejected() {
    rejects("fn take_int(x int) {}\n\nfn main() {\n    var err error\n    take_int(err)\n}\n");
}

#[test]
fn error_value_returned_from_int_fn_is_rejected() {
    rejects("fn f() int {\n    var err error\n    return err\n}\n\nfn main() { _ = f() }\n");
}

#[test]
fn int_argument_into_error_param_is_rejected() {
    rejects("fn take_error(e error) {}\n\nfn main() {\n    take_error(42)\n}\n");
}

#[test]
fn error_passed_between_error_positions_stays_accepted() {
    accepts("fn take_error(e error) {}\n\nfn f() error {\n    var err error\n    take_error(err)\n    return err\n}\n\nfn main() { _ = f() }\n");
}

#[test]
fn struct_implementing_error_stays_accepted() {
    accepts("struct myerr { msg string }\n\nfn f() error {\n    return myerr{ msg: \"x\" }\n}\n\nfn main() { _ = f() }\n");
}
