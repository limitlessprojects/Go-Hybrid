//! Regression tests for #4758: the blank identifier `_` is not a member.
//!
//! A blank struct field (`_ int`) and a method declared `fn (self t) _()`
//! are legal Go *declarations* (the field reserves layout; the method is
//! unreferenceable), so they must keep passing `hyb check` and keep being
//! emitted. But every member *lookup* of `_` is `v._ undefined` at
//! `go build`, yet `hyb check` accepted `v._`, `v._ = 2`, `point{_: 1}`,
//! `v._()`, `f := v._` and generic `box[int]._` with 0 diagnostics. The fix
//! guards the selector/literal lookups (leaving the declaration tables
//! intact for layout, comparability and method-set diagnostics), so each
//! lookup rejects with HYB-E1002 while declarations stay accepted.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects_e1002(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {:?}, got {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "verdict for {:?}: {:?}",
        src,
        r.diagnostics
    );
}

// A blank field is declared, so pre-fix the checker found `_` in its field
// table and typed the read as `int`.
#[test]
fn blank_field_read_rejects_e1002() {
    rejects_e1002(
        "\
struct point {
    x int
    _ int
}
fn main() {
    var v point
    y := v._
    _ = y
}
",
    );
}

// Assignment targets go through `PlaceKind::Field`, a different lookup path
// from expression reads.
#[test]
fn blank_field_write_rejects_e1002() {
    rejects_e1002(
        "\
struct point {
    x int
    _ int
}
fn main() {
    var v point
    v._ = 2
}
",
    );
}

#[test]
fn blank_field_in_struct_literal_rejects_e1002() {
    rejects_e1002(
        "\
struct point {
    x int
    _ int
}
fn main() {
    p := point{_: 1, x: 2}
    _ = p
}
",
    );
}

// Elided composite literals (`[]point{{...}}`) resolve their keys against the
// element type through a separate checker.
#[test]
fn blank_field_in_elided_struct_literal_rejects_e1002() {
    rejects_e1002(
        "\
struct point {
    x int
    _ int
}
fn main() {
    ps := []point{{_: 1, x: 2}}
    _ = ps
}
",
    );
}

// A method declared `_` lands in `method_decls`; no selector may find it.
#[test]
fn blank_method_call_rejects_e1002() {
    rejects_e1002(
        "\
struct point {
    x int
}
fn (self point) _() {}
fn main() {
    var v point
    v._()
}
",
    );
}

#[test]
fn blank_method_value_rejects_e1002() {
    rejects_e1002(
        "\
struct point {
    x int
}
fn (self point) _() {}
fn main() {
    var v point
    f := v._
    _ = f
}
",
    );
}

// A type-qualified base is normally a method expression (`point.norm`), not a
// value field read — the guard still has to reject `point._`.
#[test]
fn blank_method_expression_rejects_e1002() {
    rejects_e1002(
        "\
struct point {
    x int
}
fn (self point) _() {}
fn main() {
    var v point
    point._(v)
}
",
    );
}

// Generic instantiation resolves fields through `struct_field_kind` with a
// type-argument substitution; `_` must be absent there too.
#[test]
fn generic_blank_field_read_rejects_e1002() {
    rejects_e1002(
        "\
struct box[t any] {
    val t
    _ int
}
fn main() {
    var b box[int]
    y := b._
    _ = y
}
",
    );
}

// The declaration itself is legal Go: the blank field reserves layout, and
// named fields/methods stay fully usable.
#[test]
fn blank_field_declaration_with_named_read_stays_accepted() {
    accepts(
        "\
struct point {
    x int
    _ int
}
fn main() {
    var v point
    v.x = 3
    y := v.x
    _ = y
}
",
    );
}

#[test]
fn blank_method_declaration_with_named_members_stays_accepted() {
    accepts(
        "\
struct point {
    x int
    _ int
}
fn (self point) _() {}
fn (self point) norm() int {
    return self.x
}
fn main() {
    var v point
    v.x = 4
    y := v.x + v.norm()
    _ = y
}
",
    );
}
