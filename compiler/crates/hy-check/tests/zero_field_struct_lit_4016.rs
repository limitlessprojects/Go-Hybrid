//! Issue #4016 (part 1): a zero-field struct literal must reject unknown fields.
//!
//! `Checker::structs` deliberately omits zero-field structs so they never
//! classify as mutable GC structs, but `check_struct_literal` used that same
//! table as its only "is this a declared struct" source and returned early when
//! the lookup missed. `unit{bogus: 1}` therefore passed `hyb check` with 0
//! diagnostics and then failed `go build`:
//!
//! ```text
//! ./x.go:3:23: unknown field bogus in struct literal of type unit
//! ```
//!
//! The fix treats a name known to `struct_type_params` but absent from
//! `structs` as a declared struct with no fields, so the U14 unknown-name check
//! runs.
//!
//! What this file pins:
//!   1. an unknown field on a zero-field struct literal is rejected (E1002);
//!   2. a plain `unit{}` with no fields stays accepted — the fix must not turn
//!      a valid empty literal into an error;
//!   3. field access on a zero-field struct binding is untouched, which is the
//!      behaviour `value_position_struct_literal_type.rs` already pins and the
//!      reason the fix lives in `check_struct_literal` and not in the struct
//!      registration table.

use hy_check::{check_file, CheckResult, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

#[test]
fn unknown_field_on_zero_field_struct_literal_rejected() {
    // The exact repro from #4016. Before the fix: "OK (0 diagnostics)".
    let r = check(
        "struct unit {}\n\
         fn bad() {\n\
             u := unit{bogus: 1}\n\
             _ = u\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.code == "HYB-E1002" && d.message.contains("bogus")),
        "expected E1002 naming the unknown field, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn empty_zero_field_struct_literal_still_accepted() {
    // Guard against over-rejection: `unit{}` is valid Go and valid Hybrid.
    let r = check(
        "struct unit {}\n\
         fn good() {\n\
             u := unit{}\n\
             _ = u\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn generic_zero_field_struct_unknown_field_rejected() {
    // The same gap on a zero-field generic struct.
    let r = check(
        "struct unit[t any] {}\n\
         fn bad() {\n\
             u := unit[int]{bogus: 1}\n\
             _ = u\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "expected an E1002, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn non_empty_struct_literal_checks_unchanged() {
    // The ordinary struct path must behave exactly as before.
    let ok = check(
        "struct pt { x int y int }\n\
         fn good() {\n\
             p := pt{x: 1, y: 2}\n\
             _ = p\n\
         }\n",
    );
    assert_eq!(ok.verdict, Verdict::Accept, "{:?}", ok.diagnostics);

    let bad = check(
        "struct pt { x int y int }\n\
         fn bad() {\n\
             p := pt{z: 1}\n\
             _ = p\n\
         }\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject, "{:?}", bad.diagnostics);
    assert!(bad.diagnostics.iter().any(|d| d.code == "HYB-E1002"));
}
