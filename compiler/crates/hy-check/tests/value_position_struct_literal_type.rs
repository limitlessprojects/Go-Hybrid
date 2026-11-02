//! Value-position type names in composite literals must resolve.
//!
//! The literal's own `ty` routes through the expression-side `ty()` walk,
//! which resolves arity/instantiation but never asks whether the named type
//! exists — `b := box2{}` with no `struct box2` anywhere passed `hyb check`
//! with 0 diagnostics and failed `go build` with `undefined: box2` (Row 13
//! BUILDABLE). Sibling #1583/#1593/#1586 cover DECLARATION sites only; this
//! is the residual value-position root, caught fail-closed via HYB-E1002's
//! existing BUILDABLE gate. Deliberate fail-open carve-outs (unit-tested
//! below): predeclared/conversion names, the `option`/`set` prelude carriers,
//! zero-field structs, dotted foreign names, and the enclosing generic
//! signature's type parameters (`map[k]v{}` inside `fn new[k, v any]`).

use hy_check::{Verdict, check_file};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).expect("must parse");
    check_file(&ast)
}

#[test]
fn rejects_zero_value_struct_literal_of_undeclared_type() {
    let r = check("fn main() {\n    b := box2{}\n    _ = b\n}\n");
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.code == "HYB-E1002" && d.message.contains("box2")),
        "expected HYB-E1002 for undeclared `box2` in value position, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn rejects_field_literal_of_undeclared_type() {
    let r = check("fn main() {\n    b := box2{ oh: some(1) }\n    _ = b\n}\n");
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.code == "HYB-E1002" && d.message.contains("box2")),
        "got: {:?}",
        r.diagnostics
    );
}

#[test]
fn rejects_slice_literal_with_undeclared_elem() {
    let r = check("fn main() {\n    xs := []box10{}\n    _ = xs\n}\n");
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.code == "HYB-E1002" && d.message.contains("box10")),
        "got: {:?}",
        r.diagnostics
    );
}

#[test]
fn rejects_map_literal_with_undeclared_value_type() {
    let r = check("fn main() {\n    m := map[string]box11{}\n    _ = m\n}\n");
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.code == "HYB-E1002" && d.message.contains("box11")),
        "got: {:?}",
        r.diagnostics
    );
}

#[test]
fn rejects_map_literal_with_undeclared_key_type() {
    let r = check("fn main() {\n    m := map[missing]int{}\n    _ = m\n}\n");
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.code == "HYB-E1002" && d.message.contains("missing")),
        "got: {:?}",
        r.diagnostics
    );
}

#[test]
fn accepts_declared_struct_enum_alias_literals() {
    let r = check(
        "struct met { v int }\n\
         struct svc {}\n\
         enum color { red }\n\
         fn main() {\n\
             m := met{v: 3}\n\
             s := svc{}\n\
             c := color.red\n\
             d := map[string]met{\"k\": met{v: 2}}\n\
             xs := []met{met{v: 1}}\n\
             _ = m.v + s.v + d[\"k\"].v + xs[0].v + 0\n\
             _ = c\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn accepts_predeclared_and_prelude_names() {
    // `any`/`int` stay accepted: the scalar/interface composite root
    // (#1601/#1602) and the resolver's predeclared set own those, and the
    // emitter leaves them literal in value positions it understands.
    let r = check(
        "fn main() {\n\
             xs := []any{1, \"two\"}\n\
             m := map[string]any{}\n\
             n := []int{}\n\
             o := map[string]int{}\n\
             _ = len(xs) + len(m) + len(n) + len(o)\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn accepts_option_and_set_prelude_carriers() {
    let r = check(
        "fn main() {\n\
             o := some(5)\n\
             s := [3, 1]\n\
             _ = o.is_some()\n\
             _ = len(s)\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn accepts_enclosing_generic_type_parameters() {
    // `lru_cache.new[k comparable, v any]` on the spike-6 surface: the
    // composite literal `map[k]v{}` (and `[]k{}`) inside the signature body
    // must stay accepted — Go compiles it verbatim.
    let r = check(
        "struct cache[k comparable, v any] {\n\
             cap int\n\
             data map[k]v\n\
         }\n\
         \n\
         fn cache.new[k comparable, v any](cap int) cache[k, v] {\n\
             return cache[k, v]{cap: cap, data: map[k]v{}}\n\
         }\n\
         \n\
         fn main() {\n\
             c := cache.new[string, int](2)\n\
             _ = c.cap\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn rejects_type_parameter_name_outside_its_signature() {
    // The flip side: `k` is only a type parameter inside a generic
    // signature — a plain fn body that spells `map[k]v{}` with no generic
    // scope must still be caught (Go: `undefined: k`).
    let r = check(
        "fn main() {\n    m := map[k]v{}\n    _ = m\n}\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.code == "HYB-E1002" && d.message.contains("`k`")),
        "got: {:?}",
        r.diagnostics
    );
}
