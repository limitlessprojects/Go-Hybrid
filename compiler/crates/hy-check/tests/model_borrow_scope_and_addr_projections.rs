use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let file = parse_file("test.hy", src).expect("parse ok");
    check_file(&file)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn test_shadowed_inner_binding_does_not_clear_outer_borrow_binding_on_return() {
    // Outer `out` holds a borrow of stack-local `x` (`[]*int{&x}`).
    // A nested block declares a shadowing `out := []*int{}` which must NOT
    // wipe `"out"` from `st.borrow_bindings` once the inner scope pops.
    // Returning outer `out` must still emit HYB-E3104 (LOAN-ESCAPE).
    let src = r#"
fn bad_escape() []*int {
    var x int = 7
    out := []*int{&x}
    if true {
        out := []*int{}
        _ = out
    }
    return out
}
"#;
    assert_eq!(codes(src), vec!["HYB-E3104"]);
}

#[test]
fn test_shadowed_inner_binding_does_not_clear_named_result_borrow_on_bare_return() {
    // Same shadowing escape via a named result binding `out map[string]*int` and bare `return`.
    let src = r#"
fn bad_named_escape() (out map[string]*int) {
    var x int = 7
    out = map[string]*int{"a": &x}
    if true {
        out := map[string]*int{}
        _ = out
    }
    return
}
"#;
    assert_eq!(codes(src), vec!["HYB-E3104"]);
}

#[test]
fn test_collect_addr_of_roots_through_slice_field_index_projections() {
    // `collect_addr_of_roots` must walk through `Expr::Slice`, `Expr::Field`, and
    // `Expr::Index` wrappers around composite literals containing `&x` so
    // interprocedural parameter stores (`copy_first(d, ([]*int{&x[0]})[0:1])`)
    // link `d` and `x` in `may_share` and reject using `d` after `ch <- move x`
    // with HYB-E3003.
    let src_slice = r#"
fn copy_first(dst []*int, src []*int) {
    dst[0] = src[0]
}

fn bad_after_move_slice(ch chan [2]int) {
    var x [2]int = [2]int{1, 2}
    var y int = 0
    d := []*int{&y}
    copy_first(d, ([]*int{&x[0]})[0:1])
    ch <- move x
    _ = d[0]
}
"#;
    assert_eq!(codes(src_slice), vec!["HYB-E3003"]);

    let src_index = r#"
fn copy_first(dst []*int, src []*int) {
    dst[0] = src[0]
}

fn bad_after_move_index(ch chan [2]int) {
    var x [2]int = [2]int{1, 2}
    var y int = 0
    d := []*int{&y}
    copy_first(d, ([][]*int{[]*int{&x[0]}})[0])
    ch <- move x
    _ = d[0]
}
"#;
    assert_eq!(codes(src_index), vec!["HYB-E3003"]);
}

#[test]
fn test_inner_shadow_borrow_does_not_taint_clean_outer_binding_after_pop() {
    // Positive regression: if outer `out` is clean (`[]*int{}`) and an inner
    // shadowed `out` temporarily borrows local `x`, popping the inner scope
    // must remove the shadow's borrow mark so `return out` is accepted.
    let src_ok = r#"
fn ok_return() []*int {
    out := []*int{}
    if true {
        var x int = 7
        out := []*int{&x}
        _ = out
    }
    return out
}
"#;
    assert!(codes(src_ok).is_empty());
}
