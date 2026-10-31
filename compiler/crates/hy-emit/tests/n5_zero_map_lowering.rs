//! n5_zero_map_lowering.rs — emission pins for issue #645.
//!
//! SPEC-LANGUAGE N5: a Hybrid-declared map's zero value allocates on first
//! write, where Go panics on a nil map. The emitter used to lower every zero
//! map Go-exact; it now allocates zero maps where they are declared: `var`
//! locals, struct literals that leave a map field unkeyed (nested structs
//! included), and `new(T)`.
//!
//! The byte-exact runtime behaviour is pinned by the corpus program
//! `n5-zero-map-first-write.hy` (hyb/tests/spec_n5_zero_maps.rs).

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit_src(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("parses");
    emit_go(&ast).expect("emits")
}

#[test]
fn n5_var_zero_map_allocates_at_declaration() {
    let go = emit_src(
        "type tally = map[string]int\n\nfn main() {\n    var m map[string]int\n    m[\"a\"] = 1\n    var t tally\n    t[\"b\"] = 2\n    var n int\n    fmt.println(len(m) + len(t) + n)\n}\n",
    );
    assert!(
        go.contains("var m map[string]int = map[string]int{}"),
        ":\n{go}"
    );
    assert!(go.contains("var t tally = tally{}"), ":\n{go}");
    // A type whose Go zero value already is Hybrid's stays untouched.
    assert!(go.contains("var n int\n"), ":\n{go}");
}

#[test]
fn n5_struct_zero_values_allocate_map_fields() {
    let go = emit_src(
        "\
struct bag {
    label  string
    counts map[string]int
}

struct shelf {
    top bag
    n   int
}

struct plain {
    n int
}

fn main() {
    var b bag
    keyed := bag{label: \"x\"}
    given := bag{counts: map[string]int{\"k\": 1}}
    var s shelf
    p := new(bag)
    var q plain
    q2 := plain{}
    bags := []bag{{label: \"e\"}}
    fmt.println(len(b.counts) + len(keyed.counts) + len(given.counts) + len(s.top.counts) + len(p.counts) + q.n + q2.n + len(bags))
}
",
    );
    for want in [
        "var b bag = bag{counts: map[string]int{}}",
        "keyed := bag{label: \"x\", counts: map[string]int{}}",
        // A keyed map field is the literal's own value — never doubled.
        "given := bag{counts: map[string]int{\"k\": 1}}",
        "var s shelf = shelf{top: bag{counts: map[string]int{}}}",
        "p := &bag{counts: map[string]int{}}",
        "var q plain\n",
        "q2 := plain{}",
        "bags := []bag{bag{label: \"e\", counts: map[string]int{}}}",
    ] {
        assert!(go.contains(want), "missing {want:?} in:\n{go}");
    }
}

#[test]
fn n5_new_peels_parenthesized_type_operands() {
    // Issue #1615: hy-check accepts `new((bag))`; the emitter matched the
    // operand without peeling Paren and emitted raw Go `new`, leaving map
    // fields nil (panic on first write).
    let go = emit_src(
        "\
type str_map = map[string]int

struct bag {
    counts map[string]int
}

fn main() {
    p := new((bag))
    q := new(((bag)))
    m := new((str_map))
    _ = m
    fmt.println(len(p.counts) + len(q.counts))
}
",
    );
    assert!(
        go.contains("p := &bag{counts: map[string]int{}}"),
        ":\n{go}"
    );
    assert!(
        go.contains("q := &bag{counts: map[string]int{}}"),
        ":\n{go}"
    );
    assert!(go.contains("m := &str_map{}"), ":\n{go}");
}

#[test]
fn n5_leaves_generic_structs_alone() {
    // Generic struct fields need the literal's type arguments substituted;
    // they keep Go's zero value (a documented limit of this lowering).
    let go = emit_src(
        "\
struct box[t comparable] {
    m map[t]int
}

fn main() {
    b := box[int]{}
    fmt.println(len(b.m))
}
",
    );
    assert!(go.contains("b := box[int]{}"), ":\n{go}");
}
