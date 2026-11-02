//! new_result_kind_3068.rs — regression tests for issue #3068.
//!
//! `call_result_kinds` handled `new(T)` with a hardcoded
//! `OpKind::Pointer(Box::new(OpKind::Unknown))`. Because `kinds_conflict`
//! recurses into pointer element kinds and is fail-open on `Unknown`, that
//! blanket unknown made `*T` unifiable with every other `*U`: programs
//! `hyb check` accepted died at `go build` with
//! `cannot use ... (value of type *int) as *string value` — Row 13 BUILDABLE.
//!
//! Fix: `new_elem_kind` resolves the element type from `args[0]` the same way
//! `make_kind` does, so `new(int)` is `Pointer(Int)` and `new(struct)` is
//! `Pointer(Struct(..))`.

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
// Rejects — the four reach paths named in the issue
// ---------------------------------------------------------------------------

#[test]
fn rejects_mismatched_variable_initialization() {
    let r = check("package main\n\nfn main() {\n    var p *string = new(int)\n    _ = p\n}\n");
    assert_e1002(&r, "var p *string = new(int)");
}

#[test]
fn rejects_mismatched_return() {
    let r = check(
        "package main\n\nfn f() *string {\n    return new(int)\n}\n\nfn main() {\n    _ = f()\n}\n",
    );
    assert_e1002(&r, "fn f() *string { return new(int) }");
}

#[test]
fn rejects_mismatched_assignment() {
    let r = check(
        "package main\n\nfn main() {\n    var p *string\n    p = new(int)\n    _ = p\n}\n",
    );
    assert_e1002(&r, "p = new(int) where p is *string");
}

#[test]
fn rejects_mismatched_call_argument() {
    let r = check(
        "package main\n\nfn take_str_ptr(p *string) {\n    _ = p\n}\n\nfn main() {\n    take_str_ptr(new(int))\n}\n",
    );
    assert_e1002(&r, "take_str_ptr(new(int))");
}

#[test]
fn rejects_mismatched_slice_and_map_new() {
    let r = check(
        "package main\n\nfn main() {\n    var p *[]int = new([]string)\n    _ = p\n}\n",
    );
    assert_e1002(&r, "var p *[]int = new([]string)");
}

// ---------------------------------------------------------------------------
// Accepts — the fix must not over-reach
// ---------------------------------------------------------------------------

#[test]
fn accepts_matching_pointer_initialization() {
    let r = check("package main\n\nfn main() {\n    var p *string = new(string)\n    _ = p\n}\n");
    assert_accept(&r, "var p *string = new(string)");
}

#[test]
fn accepts_new_inferred_binding() {
    let r = check("package main\n\nfn main() {\n    p := new(int)\n    _ = p\n}\n");
    assert_accept(&r, "p := new(int)");
}

#[test]
fn accepts_matching_return() {
    let r = check(
        "package main\n\nfn f() *string {\n    return new(string)\n}\n\nfn main() {\n    _ = f()\n}\n",
    );
    assert_accept(&r, "fn f() *string { return new(string) }");
}
