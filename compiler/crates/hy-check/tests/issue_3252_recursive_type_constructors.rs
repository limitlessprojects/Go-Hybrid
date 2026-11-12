//! #3252 — reject recursive value types that recurse through a type
//! constructor (generic instantiation, the prelude generic `option[T]`, or
//! a type alias) instead of a plain named-struct field.
//!
//! The direct shapes (`struct node { next node }`, `[N]node`, mutual struct
//! cycles) are #1566 / #2590's territory. The shapes here were accepted with
//! zero diagnostics on main dd3a968 even though the types have infinite size
//! and Go rejects the emitted code (`invalid recursive type ... refers to
//! itself`). Detection has to expand constructors: instantiating `box[node]`
//! substitutes `t := node` into box's fields, `option[T]` carries its payload
//! by value, and aliases must be expanded (an alias denotes its target; the
//! emitted Go alias cannot refer to itself at all).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_reject(src: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

fn assert_reject_named(src: &str, name: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(
        r.diagnostics.iter().any(|d| d.message.contains(&format!(
            "invalid recursive type: {} refers to itself",
            name
        ))),
        "diagnostic should name `{}`: {:?}",
        name,
        r.diagnostics
    );
}

fn assert_accept(src: &str) {
    let r = check(src);
    assert!(error_codes(&r).is_empty(), "{:?}", r.diagnostics);
}

const BOX: &str = "\
struct box[t] {
    v t
}
";

// ---------------------------------------------------------------------------
// Rejects: the three shapes from the issue
// ---------------------------------------------------------------------------

#[test]
fn reject_recursion_through_generic_instantiation() {
    assert_reject_named(
        &format!(
            "{BOX}struct node {{\n    b box[node]\n}}\nfn main() {{\n    var n node\n    _ = n\n}}\n"
        ),
        "node",
    );
}

#[test]
fn reject_recursion_through_option_payload() {
    assert_reject_named(
        "struct node {\n    o option[node]\n}\nfn main() {\n    var n node\n    _ = n\n}\n",
        "node",
    );
}

#[test]
fn reject_alias_array_cycle() {
    assert_reject_named(
        "type bad = [3]bad\nfn main() {\n    var b bad\n    _ = b\n}\n",
        "bad",
    );
}

// ---------------------------------------------------------------------------
// Rejects: neighbouring constructor-mediated shapes
// ---------------------------------------------------------------------------

#[test]
fn reject_recursion_through_nested_instantiation() {
    assert_reject_named(
        &format!(
            "{BOX}struct node {{\n    b box[box[node]]\n}}\nfn main() {{\n    var n node\n    _ = n\n}}\n"
        ),
        "node",
    );
}

#[test]
fn reject_mutual_recursion_through_generic() {
    assert_reject(&format!(
        "{BOX}struct a {{\n    b box[b]\n}}\nstruct b {{\n    x a\n}}\nfn main() {{\n    var v a\n    _ = v\n}}\n"
    ));
}

#[test]
fn reject_alias_through_generic_instantiation() {
    assert_reject_named(
        &format!("{BOX}type nb = box[nb]\nfn main() {{\n    var n nb\n    _ = n\n}}\n"),
        "nb",
    );
}

#[test]
fn reject_generic_struct_instantiating_itself() {
    assert_reject_named(
        "struct box[t] {\n    v box[t]\n}\nfn main() {\n    var b box[int]\n    _ = b\n}\n",
        "box",
    );
}

#[test]
fn reject_mutual_alias_cycle() {
    assert_reject("type a = b\ntype b = a\nfn main() {\n    var v a\n    _ = v\n}\n");
}

// Go aliases denote their target and must expand to a finite type, so an
// alias cannot refer to itself even through an indirection (the emitter
// writes a Go alias: `type ns = []ns` is `invalid recursive type alias`
// in Go).
#[test]
fn reject_alias_slice_self_reference() {
    assert_reject_named(
        "type ns = []ns\nfn main() {\n    var n ns\n    _ = n\n}\n",
        "ns",
    );
}

// ---------------------------------------------------------------------------
// Controls: indirections break the cycle and stay legal
// ---------------------------------------------------------------------------

#[test]
fn accept_slice_recursion() {
    assert_accept("struct node {\n    kids []node\n}\nfn main() {\n    var n node\n    _ = n\n}\n");
}

#[test]
fn accept_map_recursion() {
    assert_accept(
        "struct node {\n    m map[string]node\n}\nfn main() {\n    var n node\n    _ = n\n}\n",
    );
}

#[test]
fn accept_chan_recursion() {
    assert_accept(
        "struct node {\n    ch chan node\n}\nfn main() {\n    var n node\n    _ = n\n}\n",
    );
}

#[test]
fn accept_fn_recursion() {
    assert_accept("struct node {\n    f fn() node\n}\nfn main() {\n    var n node\n    _ = n\n}\n");
}

// (No pointer-field control: `*T` fields are rejected by HYB-E3103 —
// fields never hold borrows — so a pointer is not a legal cycle-breaker
// inside a struct in Hybrid, unlike in Go.)

#[test]
fn accept_generic_nonrecursive_use() {
    assert_accept(&format!(
        "{BOX}struct node {{\n    b box[int]\n}}\nfn main() {{\n    var n node\n    _ = n\n}}\n"
    ));
}

#[test]
fn accept_option_nonrecursive_use() {
    assert_accept(
        "struct node {\n    o option[int]\n}\nfn main() {\n    var n node\n    _ = n\n}\n",
    );
}

#[test]
fn accept_generic_of_slice_recursion() {
    assert_accept(&format!(
        "{BOX}struct node {{\n    b box[[]node]\n}}\nfn main() {{\n    var n node\n    _ = n\n}}\n"
    ));
}

#[test]
fn accept_struct_field_through_alias_to_slice() {
    assert_accept(
        "type ns = []node\nstruct node {\n    f ns\n}\nfn main() {\n    var n node\n    _ = n\n}\n",
    );
}
