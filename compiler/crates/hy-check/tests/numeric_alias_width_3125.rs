//! numeric_alias_width_3125.rs — regression tests for issue #3125.
//!
//! `scalar_of_ty` returned `None` for every name present in
//! `ck.type_aliases`, treating an alias as if it carried no scalar width.
//! A type alias is an alternative SPELLING of its underlying type, so
//! `type my_i8 = int8` has width `int8`. The blanket `None` silenced the
//! whole width subsystem for alias-typed bindings, and
//! `var a my_i8 = 1; var b int64 = a` passed `hyb check` then failed
//! `go build` with `cannot use a (variable of int8 type my_i8) as int64` —
//! Row 13 BUILDABLE.
//!
//! Fix: `scalar_of_ty` follows the alias to its underlying type, with a
//! depth bound so a cyclic alias fails open instead of recursing forever.
//! The diagnostic now also names the declared spelling, not the resolved
//! width, so the message matches what Go prints.

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

fn assert_e1002(r: &hy_check::CheckResult, what: &str) {
    let cs = codes(r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 ({}), got {:?}",
        what,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

fn assert_accept(r: &hy_check::CheckResult, what: &str) {
    let cs = codes(r);
    assert!(cs.is_empty(), "expected accept ({}), got {:?}", what, cs);
    assert_eq!(r.verdict, Verdict::Accept);
}

// ---------------------------------------------------------------------------
// Rejects — alias-typed values must not silently unify with a wider width
// ---------------------------------------------------------------------------

#[test]
fn rejects_alias_source_into_wider_target() {
    let r = check(
        "package main\n\ntype my_i8 = int8\n\nfn main() {\n    var a my_i8 = 1\n    var b int64 = a\n    _ = b\n}\n",
    );
    assert_e1002(&r, "var b int64 = a where a is my_i8");
}

#[test]
fn rejects_wider_source_into_alias_target() {
    let r = check(
        "package main\n\ntype my_i8 = int8\n\nfn main() {\n    var a int64 = 1\n    var b my_i8 = a\n    _ = b\n}\n",
    );
    assert_e1002(&r, "var b my_i8 = a where a is int64");
}

#[test]
fn rejects_through_chained_alias() {
    let r = check(
        "package main\n\ntype my_i8 = int8\ntype user_id = my_i8\n\nfn main() {\n    var a user_id = 1\n    var b int64 = a\n    _ = b\n}\n",
    );
    assert_e1002(&r, "chained alias user_id -> my_i8 -> int8 into int64");
}

#[test]
fn rejects_float_alias_into_int() {
    let r = check(
        "package main\n\ntype ratio = float64\n\nfn main() {\n    var a ratio = 1.5\n    var b int64 = a\n    _ = b\n}\n",
    );
    assert_e1002(&r, "var b int64 = a where a is a float64 alias");
}

// ---------------------------------------------------------------------------
// Accepts — the fix must not over-reach
// ---------------------------------------------------------------------------

#[test]
fn accepts_alias_into_matching_width() {
    let r = check(
        "package main\n\ntype my_i8 = int8\n\nfn main() {\n    var a my_i8 = 1\n    var b int8 = a\n    _ = b\n}\n",
    );
    assert_accept(&r, "my_i8 -> int8 is the same width");
}

#[test]
fn accepts_chained_alias_into_matching_width() {
    let r = check(
        "package main\n\ntype my_i8 = int8\ntype user_id = my_i8\n\nfn main() {\n    var a user_id = 1\n    var b int8 = a\n    _ = b\n}\n",
    );
    assert_accept(&r, "chained alias -> int8 is the same width");
}
#[test]
fn accepts_plain_numeric_declaration() {
    let r = check("package main\n\nfn main() {\n    var a int8 = 1\n    var b int8 = a\n    _ = b\n}\n");
    assert_accept(&r, "non-alias path unchanged");
}
