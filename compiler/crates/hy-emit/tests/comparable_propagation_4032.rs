//! Issue #4032 (part 1): `comparable` must propagate through user generic
//! struct fields.
//!
//! `emit_struct` computed each struct's `comparable` params with
//! `collect_comparable_in_ty`, which is purely syntactic: it knows `map[k]v`
//! and `set[t]` demand `comparable`, but it stops at a user generic struct. So
//!
//! ```hybrid
//! struct table[t] { m map[t]int }
//! struct outer[k] { inner table[k] }
//! ```
//!
//! emitted `type table[t comparable]` (correct) and `type outer[k any]`
//! (wrong), and Go rejected the result:
//!
//! ```text
//! ./main.go:8:14: k does not satisfy comparable
//! ```
//!
//! What this file pins:
//!   1. the two-level case from the report now promotes `comparable` to `outer`;
//!   2. a three-level chain (`a -> b -> c`) also propagates, since the fix
//!      iterates to a fixpoint rather than one hop;
//!   3. a generic struct whose param is genuinely unconstrained still emits
//!      `any` — the promotion must not over-fire and start demanding
//!      `comparable` everywhere;
//!   4. an unrelated generic struct next to a demanding one keeps `any`, so the
//!      fixpoint does not leak the demand across structs.

use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let ast = parse_file("pkg.hy", src).expect("parses");
    hy_emit::emit_go_package(&ast, "main", "pkg.hy").expect("emits package")
}

#[test]
fn comparable_propagates_through_one_struct_field() {
    let go = emit(
        "package main\n\n\
         struct table[t] {\n    m map[t]int\n}\n\n\
         struct outer[k] {\n    inner table[k]\n}\n\n\
         fn main() {}\n",
    );
    assert!(
        go.contains("type table[t comparable] struct"),
        "table keeps comparable:\n{go}"
    );
    assert!(
        go.contains("type outer[k comparable] struct"),
        "outer must inherit comparable through its table[k] field:\n{go}"
    );
}

#[test]
fn comparable_propagates_through_a_two_hop_chain() {
    // a -> b -> c: the fixpoint must reach `a`, not stop after one hop.
    let go = emit(
        "package main\n\n\
         struct c0[x] {\n    m map[x]int\n}\n\n\
         struct b0[y] {\n    inner c0[y]\n}\n\n\
         struct a0[z] {\n    inner b0[z]\n}\n\n\
         fn main() {}\n",
    );
    for want in [
        "type c0[x comparable] struct",
        "type b0[y comparable] struct",
        "type a0[z comparable] struct",
    ] {
        assert!(go.contains(want), "missing {want:?} in:\n{go}");
    }
}

#[test]
fn unconstrained_generic_still_emits_any() {
    // Guard against over-firing: a param used only as a plain value keeps `any`.
    let go = emit(
        "package main\n\n\
         struct holder[t] {\n    v t\n}\n\n\
         fn main() {}\n",
    );
    assert!(
        go.contains("type holder[t any] struct"),
        "a value-only param must stay any:\n{go}"
    );
}

#[test]
fn demand_does_not_leak_to_an_unrelated_struct() {
    // `table` demands comparable; `holder` sits beside it and must not inherit
    // anything. This catches a fixpoint that unions across the whole table.
    let go = emit(
        "package main\n\n\
         struct table[t] {\n    m map[t]int\n}\n\n\
         struct holder[u] {\n    v u\n}\n\n\
         struct outer[k] {\n    inner table[k]\n    extra holder[k]\n}\n\n\
         fn main() {}\n",
    );
    assert!(
        go.contains("type holder[u any] struct"),
        "unrelated struct keeps any:\n{go}"
    );
    assert!(
        go.contains("type outer[k comparable] struct"),
        "outer still inherits from table:\n{go}"
    );
}

#[test]
fn non_generic_structs_untouched() {
    let go = emit(
        "package main\n\n\
         struct plain {\n    m map[string]int\n}\n\n\
         fn main() {}\n",
    );
    assert!(
        go.contains("type plain struct"),
        "a non-generic struct keeps its plain declaration:\n{go}"
    );
}
