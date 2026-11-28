//! Issue #4026 (part 1): a direct struct-field address-of is a slot loan too.
//!
//! `is_slot_loan_expr` delegated to `walk::addr_of_index_base_path`, which
//! requires an `Expr::Index` hop. A **field-only** address-of therefore produced
//! no path, the loan was never marked, and a pointer-to-pointer rebind slipped
//! past E3104:
//!
//! ```hybrid
//! struct bal  { v int }
//! struct acct { bal bal }
//!
//! fn bad(a acct) {
//!     f := &a.bal   // not marked as a slot loan
//!     p := &f        // should be E3104, passed with 0 diagnostics
//!     _ = p
//! }
//! ```
//!
//! The index form was already rejected, so this was a pure asymmetry between two
//! spellings of the same loan.
//!
//! What this file pins:
//!   1. the field form now rejects the pointer-to-pointer rebind (E3104);
//!   2. the field form alone (`f := &a.bal`, no second `&`) stays accepted — the
//!      ordinary loan is legal and the fix must not turn it into an error;
//!   3. the index form still rejects, so the original guard is untouched;
//!   4. an `&x` with **no** field hop is still an ordinary pointer, not a slot
//!      loan — `p := &x` on a plain binding must not start firing E3104.

use hy_check::{check_file, CheckResult, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn has_e3104(r: &CheckResult) -> bool {
    r.diagnostics.iter().any(|d| d.code == "HYB-E3104")
}

#[test]
fn field_slot_loan_pointer_rebind_rejected() {
    // The exact repro from part 1.
    let r = check(
        "struct bal { v int }\n\
         struct acct { bal bal }\n\
         \n\
         fn bad(a acct) {\n\
             f := &a.bal\n\
             p := &f\n\
             _ = p\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        has_e3104(&r),
        "expected E3104 for the field slot-loan rebind, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn lone_field_slot_loan_accepted() {
    // Guard against over-rejection: taking the loan itself is the documented
    // legal shape.
    let r = check(
        "struct bal { v int }\n\
         struct acct { bal bal }\n\
         \n\
         fn good(a acct) {\n\
             f := &a.bal\n\
             _ = f\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn index_slot_loan_still_rejected() {
    // The pre-existing guard must be untouched by this change.
    let r = check(
        "fn bad(m map[string]int) {\n\
             n := &m[\"a\"]\n\
             p := &n\n\
             _ = p\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(has_e3104(&r), "expected E3104, got: {:?}", r.diagnostics);
}

#[test]
fn plain_pointer_is_not_a_slot_loan() {
    // `&x` with no field hop is an ordinary pointer. `p := &x` there is not a
    // pointer-to-pointer slot rebind and must not fire E3104 — that is why
    // addr_of_field_base_path requires at least one Field hop.
    let r = check(
        "fn good(x int) {\n\
             p := &x\n\
             _ = p\n\
         }\n",
    );
    assert!(
        !has_e3104(&r),
        "a plain pointer must not be treated as a slot loan: {:?}",
        r.diagnostics
    );
}

#[test]
fn nested_field_slot_loan_rejected() {
    // A two-hop field chain bottoms out at the same ident root.
    let r = check(
        "struct inner { v int }\n\
         struct mid { i inner }\n\
         struct outer { m mid }\n\
         \n\
         fn bad(o outer) {\n\
             f := &o.m.i\n\
             p := &f\n\
             _ = p\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(has_e3104(&r), "expected E3104, got: {:?}", r.diagnostics);
}
