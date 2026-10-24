//! recursive_value_type_1566.rs — a struct that contains itself BY VALUE
//! (issue #1566).
//!
//! Go rejects `struct node { next node }` at the type declaration
//! (`invalid recursive type: node refers to itself`); nothing in the checker
//! built the containment graph, so `hyb check` accepted it with 0 diagnostics
//! and the emitted module failed `go build` (GUARANTEES row 13, BUILDABLE).
//!
//! The walk follows only by-value containment: direct struct fields, array
//! elements, `option[T]` payloads and type aliases. Slice, map, channel and
//! function indirections break the cycle and must stay accepted.

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

fn assert_rejects(src: &str, what: &str) {
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {what}, got {cs:?}"
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
// Reject shapes — every one is `invalid recursive type` in Go.
// ---------------------------------------------------------------------------

#[test]
fn direct_self_reference_rejects() {
    assert_rejects(
        "struct node {\n    next node\n}\n\nfn main() {\n    var n node\n    fmt.println(n.next.next)\n}\n",
        "struct node { next node }",
    );
}

#[test]
fn array_self_reference_rejects() {
    assert_rejects(
        "struct node {\n    kids [2]node\n}\n\nfn main() {\n    var n node\n    fmt.println(len(n.kids))\n}\n",
        "struct node { kids [2]node }",
    );
}

#[test]
fn two_struct_cycle_rejects() {
    assert_rejects(
        "struct a {\n    b b\n}\n\nstruct b {\n    a a\n}\n\nfn main() {\n    var x a\n    fmt.println(x.b.a.b.a)\n}\n",
        "a -> b -> a",
    );
}

#[test]
fn alias_cycle_rejects() {
    assert_rejects(
        "type n = node\n\nstruct node {\n    x n\n}\n\nfn main() {\n    var v node\n    fmt.println(v.x.x)\n}\n",
        "struct node { x n } with `type n = node`",
    );
}

#[test]
fn option_payload_cycle_rejects() {
    assert_rejects(
        "struct node {\n    o option[node]\n}\n\nfn main() {\n    var n node\n    fmt.println(n.o)\n}\n",
        "struct node { o option[node] }",
    );
}

// ---------------------------------------------------------------------------
// Alias leg — an ALIAS that expands to itself. Unlike a defined type, an alias
// is a transparent renaming, so Go rejects the cycle through ANY constructor:
// `type a = []a`, `type a = map[K]a`, `type a = *a`, `type a = a` and
// `type a = b` / `type b = a` are all `invalid recursive type`.
// ---------------------------------------------------------------------------

#[test]
fn self_alias_rejects() {
    assert_rejects(
        "type a = a\n\nfn main() {\n    var x a\n    fmt.println(x)\n}\n",
        "`type a = a`",
    );
}

#[test]
fn two_alias_cycle_rejects() {
    assert_rejects(
        "type a = b\n\ntype b = a\n\nfn main() {\n    var x a\n    fmt.println(x)\n}\n",
        "`type a = b` / `type b = a`",
    );
}

#[test]
fn alias_through_slice_rejects() {
    assert_rejects(
        "type slicy = []slicy\n\nfn main() {\n    var x slicy\n    fmt.println(x)\n}\n",
        "`type slicy = []slicy` (a slice does NOT break an alias cycle)",
    );
}

#[test]
fn alias_through_map_rejects() {
    assert_rejects(
        "type mappy = map[string]mappy\n\nfn main() {\n    var x mappy\n    fmt.println(len(x))\n}\n",
        "`type mappy = map[string]mappy`",
    );
}

#[test]
fn alias_through_array_and_pointer_rejects() {
    let src = "type arr = [2]arr\n\ntype ptr = *ptr\n\nfn main() {\n    var a arr\n    fmt.println(a)\n    var p ptr\n    fmt.println(p)\n}\n";
    assert_rejects(src, "`type arr = [2]arr` and `type ptr = *ptr`");
}

#[test]
fn alias_cycle_through_array_rejects() {
    assert_rejects(
        "type a = b\n\ntype b = [2]a\n\nfn main() {\n    var x a\n    fmt.println(x)\n}\n",
        "`type a = b` / `type b = [2]a`",
    );
}

// ---------------------------------------------------------------------------
// Controls — indirections break the cycle and are legal Go.
// ---------------------------------------------------------------------------

#[test]
fn slice_map_and_chan_self_reference_accept() {
    let src = r#"struct slicy {
    kids []slicy
}

struct mappy {
    kids map[string]mappy
}

struct channy {
    ch chan channy
}

fn main() {
    var a slicy
    fmt.println(len(a.kids))
    var b mappy
    fmt.println(len(b.kids))
    var c channy
    fmt.println(c.ch)
}
"#;
    assert_accepts(src, "slice / map / chan self-reference");
}

#[test]
fn plain_structs_accept() {
    assert_accepts(
        "struct p {\n    a int\n    s string\n}\n\nstruct q {\n    p p\n    n [2]int\n}\n\nfn main() {\n    var v q\n    fmt.println(v.p.a)\n}\n",
        "non-recursive nested structs",
    );
}

#[test]
fn alias_to_a_non_recursive_type_accept() {
    assert_accepts(
        "type row = []int\n\nfn main() {\n    var r row\n    r = append(r, 1)\n    fmt.println(len(r))\n}\n",
        "`type row = []int`",
    );
}

#[test]
fn alias_chain_to_a_defined_type_accept() {
    assert_accepts(
        "type tally = map[string]int\n\ntype counts = tally\n\nfn main() {\n    m := counts{}\n    m[\"a\"] = 1\n    fmt.println(len(m))\n}\n",
        "`type counts = tally` where tally = map[string]int",
    );
}
