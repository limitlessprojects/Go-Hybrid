//! make_and_set_lit_type_alias_3601.rs: regression tests for #3601.
//!
//! Verifies that `make(...)` and set composite literals properly resolve
//! type aliases via `binding_kind_of_ty` in `hy-emit`, preserving channel single-var
//! iteration, map key binding in for-in loops, and set method lowering (`.len()`, `.remove()`, `.m`).

#[test]
fn make_and_set_lit_resolve_type_aliases_3601() {
    let src = r#"
type int_set = set[int]
type sym_map = map[string]int
type int_chan = chan int

fn run() {
    ch := make(int_chan, 1)
    for x in ch {
        _ = x
    }
    for y in make(int_chan, 1) {
        _ = y
    }
    ch2 := make((int_chan), 2)
    for z in ch2 {
        _ = z
    }
    m := make(sym_map)
    for k in m {
        _ = k
    }
    for k2 in make(sym_map) {
        _ = k2
    }
    m2 := make((sym_map))
    for k3 in m2 {
        _ = k3
    }
    _ = len(int_set{})
    delete(int_set{}, 42)
    s := int_set{}
    for v in s {
        _ = v
    }
    for v2 in (int_set{}) {
        _ = v2
    }
}
"#;
    let ast = hy_syntax::parse_file("test.hy", src).expect("must parse");
    let go = hy_emit::emit_go(&ast).expect("must emit go");

    // Channels in Go only permit one range variable: `for x := range ch`
    assert!(
        go.contains("for x := range ch"),
        "expected single-var channel range for ch, got:\n{go}"
    );
    assert!(
        go.contains("for y := range make(int_chan, 1)"),
        "expected single-var channel range for make(int_chan, 1), got:\n{go}"
    );
    assert!(
        go.contains("for z := range ch2"),
        "expected single-var channel range for ch2, got:\n{go}"
    );

    // Single-var map range in Hybrid binds the key: `for k := range m`
    assert!(
        go.contains("for k := range m"),
        "expected key range for map m, got:\n{go}"
    );
    assert!(
        go.contains("for k2 := range make(sym_map)"),
        "expected key range for make(sym_map), got:\n{go}"
    );
    assert!(
        go.contains("for k3 := range m2"),
        "expected key range for map m2, got:\n{go}"
    );

    // Set operations lower to prelude set methods
    assert!(
        go.contains("int_set{}.len()"),
        "expected int_set{{}}.len(), got:\n{go}"
    );
    assert!(
        go.contains("int_set{}.remove(42)"),
        "expected int_set{{}}.remove(42), got:\n{go}"
    );
    assert!(
        go.contains("for v := range s.m"),
        "expected set iteration over .m for s, got:\n{go}"
    );
    assert!(
        go.contains("for v2 := range (int_set{}).m"),
        "expected set iteration over .m for (int_set{{}}), got:\n{go}"
    );
}
