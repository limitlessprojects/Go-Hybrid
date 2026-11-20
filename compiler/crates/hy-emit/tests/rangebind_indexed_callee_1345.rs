use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn indexed_callee_rangebind_preserves_binders_and_blanks_fields_1345() {
    let src = r#"package main
struct rec { fns []fn(int) int }
fn main() {
    fns := []fn(int) int{fn(x int) int { return x }}
    for i, _ in fns { fmt.println(fns[i](10)) }
    groups := [][]fn(int) int{fns}
    for _, fns_g in groups { fmt.println(fns_g[0](10)) }
    row := []int{0}
    for r_idx, _ in row { fmt.println(fns[row[r_idx]](1)) }
    r := rec{fns: fns}
    for fi, _ in fns { fmt.println(r.fns[fi](1)) }
    pairs := []int{1}
    for fns_g, x in pairs { _ = r.fns[0](x) }
}
"#;
    let go = emit_go(&parse_file("test.hy", src).unwrap()).unwrap();
    assert!(go.contains("for i := range fns {") || go.contains("for i, _ := range fns {"));
    assert!(go.contains("for _, fns_g := range groups {"));
    assert!(go.contains("for r_idx := range row {") || go.contains("for r_idx, _ := range row {"));
    assert!(go.contains("for fi := range fns {") || go.contains("for fi, _ := range fns {"));
    assert!(go.contains("for _, x := range pairs {"));
}

#[test]
fn indexed_callee_match_guard_and_shadow_hazard_1345() {
    let src = r#"package main
fn main() {
    preds := []fn() bool{fn() bool { return true }}
    opt := some(0)
    match opt {
        some(i) if preds[i](): { fmt.println(i) }
        none: {}
    }
    fns := []fn(int) int{fn(x int) int { return x }}
    opts := []option[int]{some(1)}
    for i, opt_item in opts {
        match opt_item {
            some(i) if i > 0: { fmt.println(i) }
            some(x): { fmt.println(fns[i](x)) }
            none: {}
        }
    }
}
"#;
    let go = emit_go(&parse_file("test.hy", src).unwrap()).unwrap();
    assert!(go.contains("i := ") && go.contains("if preds[i]() {"));
    assert!(!go.contains("for _, opt_item := range opts {"));
}
