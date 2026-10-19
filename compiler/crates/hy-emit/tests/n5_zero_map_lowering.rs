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
fn n5_generic_structs_substitute_type_args_and_allocate_map_fields() {
    let go = emit_src(
        "\
struct box[t comparable] {
    m map[t]int
}

struct table[k, v] {
    items    map[k]v
    fallback v
}

struct holder[t] {
    val t
}

struct bag {
    counts map[string]int
}

fn main() {
    var b1 box[int]
    b2 := box[u8]{}
    p := new(box[u8])
    var tbl table[string, bag]
    var h holder[map[string]u8]
    p_pair := new(table[u8, i16])
    fmt.println(len(b1.m) + len(b2.m) + len(p.m) + len(tbl.items) + len(tbl.fallback.counts) + len(h.val) + len(p_pair.items))
}
",
    );
    for want in [
        "var b1 box[int] = box[int]{m: map[int]int{}}",
        "b2 := box[uint8]{m: map[uint8]int{}}",
        "p := &box[uint8]{m: map[uint8]int{}}",
        "var tbl table[string, bag] = table[string, bag]{items: map[string]bag{}, fallback: bag{counts: map[string]int{}}}",
        "var h holder[map[string]uint8] = holder[map[string]uint8]{val: map[string]uint8{}}",
        "p_pair := &table[uint8, int16]{items: map[uint8]int16{}}",
    ] {
        assert!(go.contains(want), "missing {want:?} in:\n{go}");
    }
}

#[test]
fn generic_struct_and_fn_infer_transitive_method_and_equality_comparable_722() {
    let go = emit_src(
        "\
struct table[k, v] {
    items map[k]v
}

struct wrapper[a, b] {
    inner table[a, b]
}

struct set_buf[e] {
    items []e
}

fn (s *set_buf[x]) unique() map[x]bool {
    var m map[x]bool
    for _, v in s.items {
        m[v] = true
    }
    return m
}

fn table.new[k, v]() table[k, v] {
    var t table[k, v]
    return t
}

fn same[t](x t, y t) bool {
    return x == y
}

fn main() {
    w := wrapper[string, u8]{}
    w.inner.items[\"a\"] = 1
    t := table.new[string, int]()
    t.items[\"b\"] = 2
    fmt.println(len(w.inner.items) + len(t.items), same(1, 1))
}
",
    );
    for want in [
        "type table[k comparable, v any] struct {",
        "type wrapper[a comparable, b any] struct {",
        "type set_buf[e comparable] struct {",
        "func table_new[k comparable, v any]() table[k, v] {",
        "var t table[k, v] = table[k, v]{items: map[k]v{}}",
        "func same[t comparable](x t, y t) bool {",
        "w := wrapper[string, uint8]{inner: table[string, uint8]{items: map[string]uint8{}}}",
    ] {
        assert!(go.contains(want), "missing {want:?} in:\n{go}");
    }
}
