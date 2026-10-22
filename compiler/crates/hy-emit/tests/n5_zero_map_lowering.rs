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

// ---------------------------------------------------------------------------
// Allocation sites (the #645 follow-up): `make([]T, n)` elements and `[N]T`
// arrays are zero values too — they used to keep Go's nil map.
// ---------------------------------------------------------------------------

#[test]
fn n5_make_slice_allocates_every_element() {
    let go = emit_src(
        "\
struct bag {
    counts map[string]int
}

struct plain {
    n int
}

type bags = []bag

fn main() {
    a := make([]bag, 2)
    b := make([]bag, 0, 4)
    c := make([]map[string]int, 1)
    d := make(bags, 2)
    var n u8 = 2
    e := make([]bag, n)
    f := make([]int, 3)
    g := make([]plain, 2)
    h := make(chan bag, 1)
    fmt.println(len(a) + len(b) + len(c) + len(d) + len(e) + len(f) + len(g) + cap(h))
}
",
    );
    for want in [
        "a := hy_n5_make(func() bag { return bag{counts: map[string]int{}} }, 2)",
        // The capacity is allocated too: a reslice inside it meets N5 values.
        "b := hy_n5_make(func() bag { return bag{counts: map[string]int{}} }, 0, 4)",
        "c := hy_n5_make(func() map[string]int { return map[string]int{} }, 1)",
        // A slice alias resolves to its element type.
        "d := hy_n5_make(func() bag { return bag{counts: map[string]int{}} }, 2)",
        // Go's make takes any integer type; the helper takes int.
        "e := hy_n5_make(func() bag { return bag{counts: map[string]int{}} }, int(n))",
        // Element types whose Go zero value already is Hybrid's: untouched.
        "f := make([]int, 3)",
        "g := make([]plain, 2)",
        "h := make(chan bag, 1)",
        "func hy_n5_make[t any](zero func() t, n int, c ...int) []t {",
    ] {
        assert!(go.contains(want), "missing {want:?} in:\n{go}");
    }
}

#[test]
fn n5_make_prelude_is_emitted_only_when_used() {
    let go = emit_src("fn main() {\n    a := make([]int, 3)\n    fmt.println(len(a))\n}\n");
    assert!(go.contains("a := make([]int, 3)"), ":\n{go}");
    assert!(!go.contains("hy_n5_make"), "prelude leaked:\n{go}");
}

#[test]
fn n5_arrays_allocate_every_element() {
    let go = emit_src(
        "\
struct bag {
    counts map[string]int
}

struct shelf {
    slots [2]bag
}

const k = 3

fn main() {
    var a [2]bag
    b := [2]bag{}
    var c [k]bag
    d := [3]bag{bag{}}
    var s shelf
    var e [2]int
    f := [2]int{}
    fmt.println(len(a) + len(b) + len(c) + len(d) + len(s.slots) + len(e) + len(f))
}
",
    );
    const FILL2: &str = "func() (hyArr [2]bag) { for i := range hyArr { hyArr[i] = bag{counts: map[string]int{}} }; return hyArr }()";
    for want in [
        &format!("var a [2]bag = {FILL2}"),
        &format!("b := {FILL2}"),
        // A const length is fine: the IIFE ranges over the array.
        "var c [k]bag = func() (hyArr [k]bag) { for i := range hyArr { hyArr[i] = bag{counts: map[string]int{}} }; return hyArr }()",
        // A partial literal pads its trailing zero elements.
        "d := [3]bag{bag{counts: map[string]int{}}, bag{counts: map[string]int{}}, bag{counts: map[string]int{}}}",
        // An array field of a zero struct goes through the same table.
        &format!("var s shelf = shelf{{slots: {FILL2}}}"),
        // Element types whose Go zero value already is Hybrid's: untouched.
        "var e [2]int\n",
        "f := [2]int{}",
    ] {
        assert!(go.contains(want), "missing {want:?} in:\n{go}");
    }
}
