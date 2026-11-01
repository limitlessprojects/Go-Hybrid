//! receiver_type_declared_1575.rs — a method receiver base type that names
//! nothing (issue #1575).
//!
//! `declare_method` (pass 1) owns the E2007 arms for primitives, aliases,
//! pointer-to-pointer and anonymous receiver types. It has no arm for a name
//! that resolves to nothing, and it CANNOT have one: pass 1 sees declarations
//! in source order, so a legal forward reference (`fn (x *later) …` above
//! `struct later`) is not registered yet. `hyb check` therefore accepted
//!
//!   fn (x *missing) m()          -> go build: undefined: missing
//!   fn (x *serde_json.value) m() -> go build: cannot define new methods
//!                                   on non-local type
//!
//! (GUARANTEES row 13, BUILDABLE).
//!
//! The pass-2 walk accepts a struct (including a zero-field struct, which the
//! shape table skips), an enum, and a generic struct receiver; it leaves the
//! pass-1 arms untouched.

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

fn assert_e2007(src: &str, what: &str) {
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E2007"),
        "expected HYB-E2007 for {what}, got {cs:?}"
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {what}");
}

fn assert_accepts(src: &str, what: &str) {
    let r = check(src);
    let cs = codes(&r);
    assert!(cs.is_empty(), "expected ACCEPT for {what}, got {cs:?}");
    assert_eq!(r.verdict, Verdict::Accept, "verdict for {what}");
}

// ---------------------------------------------------------------------------
// Reject shapes.
// ---------------------------------------------------------------------------

#[test]
fn pointer_receiver_undeclared_rejects() {
    assert_e2007(
        "fn (x *missing) m() int {\n    return 1\n}\n\nfn main() {\n    fmt.println(\"hi\")\n}\n",
        "`fn (x *missing) m()`",
    );
}

#[test]
fn value_receiver_undeclared_rejects() {
    assert_e2007(
        "fn (x missing) m() int {\n    return 1\n}\n\nfn main() {\n    fmt.println(\"hi\")\n}\n",
        "`fn (x missing) m()`",
    );
}

#[test]
fn foreign_receiver_rejects() {
    assert_e2007(
        "fn (x *serde_json.value) m() int {\n    return 1\n}\n\nfn main() {\n    fmt.println(\"hi\")\n}\n",
        "a method on a foreign type",
    );
}

// ---------------------------------------------------------------------------
// Controls.
// ---------------------------------------------------------------------------

#[test]
fn forward_receiver_declaration_accepts() {
    // The receiver is declared AFTER the method. This is legal Go and builds
    // today; it is also why the check must run post-collection.
    assert_accepts(
        "fn (x *later) m() int {\n    return x.a\n}\n\nstruct later {\n    a int\n}\n\nfn main() {\n    var v later\n    fmt.println(v.m())\n}\n",
        "`fn (x *later) m()` above `struct later`",
    );
}

#[test]
fn empty_struct_receiver_accepts() {
    // A zero-field struct is skipped by the shape table (#722) but it IS a
    // declared type and Go allows methods on it.
    assert_accepts(
        "struct empty {}\n\nfn (x *empty) m() int {\n    return 1\n}\n\nfn main() {\n    var e empty\n    fmt.println(e.m())\n}\n",
        "a method on `struct empty {}`",
    );
}

#[test]
fn generic_struct_receiver_accepts() {
    assert_accepts(
        "struct box[t] {\n    v t\n}\n\nfn (b *box[t]) get() t {\n    return b.v\n}\n\nfn main() {\n    var b box[int]\n    b.v = 1\n    fmt.println(b.get())\n}\n",
        "a generic struct receiver",
    );
}

#[test]
fn enum_receiver_accepts() {
    // An enum is a defined type: Go allows methods on it (the emitter has a
    // separate, unrelated enum limitation).
    assert_accepts(
        "enum color {\n    red,\n    blue\n}\n\nfn (c *color) name() int {\n    return 1\n}\n\nfn main() {\n    fmt.println(\"hi\")\n}\n",
        "a method on a declared enum",
    );
}

#[test]
fn alias_receiver_stays_with_pass_one_e2007() {
    // The pass-1 arm owns this shape; the pass-2 walk must not add a second
    // E2007 for it.
    let r = check("type small = int\n\nfn (x *small) m() int {\n    return 1\n}\n\nfn main() {\n    fmt.println(\"hi\")\n}\n");
    let e2007: Vec<_> = r
        .diagnostics
        .iter()
        .filter(|d| d.code == "HYB-E2007")
        .collect();
    assert_eq!(e2007.len(), 1, "expected exactly one E2007: {:?}", r.diagnostics);
    assert!(
        e2007[0]
            .notes
            .iter()
            .any(|n| n.contains("type alias")),
        "expected the pass-1 alias note, got {:?}",
        e2007[0].notes
    );
}
