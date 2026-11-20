use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;
fn check_codes(src: &str) -> Vec<String> {
    check_file(&parse_file("test.hy", src).expect("parse")).diagnostics.into_iter().map(|d| d.code).collect()
}
fn assert_accept(src: &str) {
    let out = check_file(&parse_file("test.hy", src).expect("parse"));
    assert_eq!(out.verdict, Verdict::Accept, "expected Accept, got {:?}", out.diagnostics);
}
#[test]
fn reject_auto_addressed_call_and_method_borrow_escape_920() {
    let src_auto_addr_call = r#"struct box { v int }
fn id_ptr(p *box) *box { return p }
fn (b *box) self_ptr() *box { return b }
fn leak_call() *box {
    b := box{v: 1}
    return id_ptr(b)
}
fn put_box(dst []*box, p *box) { dst[0] = p }
fn (b *box) getter() fn() int { return fn() int { return b.v } }
fn leak_method_bound() *box {
    b := box{v: 1}
    p := b.self_ptr()
    return p
}
fn leak_auto_out_param() []*box {
    b := box{v: 1}
    dst := make([]*box, 1)
    put_box(dst, b)
    return dst
}
fn leak_closure_ret() fn() int {
    b := box{v: 1}
    return b.getter()
}"#;
    assert_eq!(check_codes(src_auto_addr_call), vec!["HYB-E3104"; 4]);
    assert_accept(r#"struct box { v int }
struct bag { items []int }
struct report { items []int }
fn id_ptr(p *box) *box { return p }
fn (b *box) get_v() int { return b.v }
fn (b box) val_v() int { return b.v }
fn (b *bag) build() report { return report{items: b.items} }
fn ok_use(ext *box) int {
    b := box{v: 7}
    p := id_ptr(b)
    _ = id_ptr(ext)
    bg := bag{items: []int{1, 2}}
    r := bg.build()
    return p.v + b.get_v() + b.val_v() + len(r.items)
}
struct tree_node {
    v int
    kids []tree_node
}
fn (n *tree_node) shallow() tree_node { return tree_node{v: n.v, kids: make([]tree_node, 0)} }
fn ok_recursive_val() tree_node {
    n := tree_node{v: 1, kids: make([]tree_node, 0)}
    return n.shallow()
}"#);
}
#[test]
fn reject_range_match_copy_and_out_param_borrow_escape_and_accept_value_copy_920() {
    let src_iter_match_copy = r#"fn put_ptr(dst []*int, p *int) { dst[0] = p }
fn leak_iter(fb *int) *int {
    x := 1
    ptrs := []*int{&x}
    for _, p in ptrs { return p }
    return fb
}
fn leak_nested() []*int {
    x := 1
    rows := [][]*int{[]*int{&x}}
    for _, row in rows { return row }
    return make([]*int, 0)
}
fn leak_match(fb *int) *int {
    x := 1
    opt := some(&x)
    match opt { some(p) => { return p } none => { return fb } }
}
fn leak_match_nested() []*int {
    x := 1
    opt := some([]*int{&x})
    match opt { some(xs) => { return xs } none => { return make([]*int, 0) } }
}
fn leak_copy() []*int {
    x := 1
    src := []*int{&x}
    dst := make([]*int, 1)
    copy(dst, src)
    return dst
}
fn leak_out_param() []*int {
    x := 1
    dst := make([]*int, 1)
    put_ptr(dst, &x)
    return dst
}"#;
    assert_eq!(check_codes(src_iter_match_copy), vec!["HYB-E3104"; 6]);
    assert_accept(r#"fn consume(xs []int) { _ = xs }
fn snapshot() []int {
    arr := [3]int{1, 2, 3}
    dst := make([]int, 3)
    copy(dst, arr[:])
    return dst
}
fn copy_param_ptrs(a *int, b *int) []*int {
    arr := [2]*int{a, b}
    dst := make([]*int, 2)
    copy(dst, arr[:])
    return dst
}
fn handoff(src []int) {
    dst := make([]int, 3)
    copy(dst, src)
    task consume(move dst)
}
fn ok_iter_param(ptrs []*int, fb *int) *int {
    for _, p in ptrs { return p }
    opt := some(fb)
    match opt { some(q) => { return q } none => { return fb } }
}"#);
}
#[test]
fn scope_pop_restores_borrow_bindings_after_shadowed_range_and_match_920() {
    let src_shadow_neg = r#"fn leak_after_range_shadow() *int {
    x := 1
    p := &x
    for _, p in []int{1, 2} { _ = p }
    return p
}
fn leak_after_match_shadow() *int {
    x := 1
    p := &x
    opt := some(42)
    match opt { some(p) => { _ = p } none => {} }
    return p
}
fn leak_after_assign_then_shadow(ext *int) *int {
    var out *int = ext
    x := 1
    if true {
        out = &x
        out := 0
        _ = out
    }
    return out
}"#;
    assert_eq!(check_codes(src_shadow_neg), vec!["HYB-E3104", "HYB-E3104", "HYB-E3104"]);
    assert_accept(r#"fn ok_after_range_shadow(q *int) *int {
    x := 1
    for _, q in []*int{&x} { _ = q }
    return q
}
fn ok_after_match_shadow(q *int) *int {
    x := 1
    opt := some(&x)
    match opt { some(q) => { _ = q } none => {} }
    return q
}"#);
}
#[test]
fn reject_index_assigned_borrow_move_race_and_accept_owned_move_920() {
    let src_index_borrow_race = r#"struct stats { count int }
fn worker(slots []*stats) { slots[0].count = 99 }
fn race_local() {
    s := stats{count: 1}
    slots := make([]*stats, 1)
    slots[0] = &s
    task worker(move slots)
    s.count = 2
}
fn non_owned_param(s stats) {
    slots := make([]*stats, 1)
    slots[0] = &s
    task worker(move slots)
}"#;
    assert_eq!(check_codes(src_index_borrow_race), vec!["HYB-E3003", "HYB-E3004"]);
    assert_accept(r#"struct stats { count int }
fn worker(slots []stats) { _ = slots }
fn clean_handoff() {
    s := stats{count: 1}
    slots := make([]stats, 1)
    slots[0] = s
    task worker(move slots)
}"#);
}
#[test]
fn reject_closure_captured_and_multivalue_borrow_escapes_and_field_store_920() {
    let src_closure = r#"struct stats { count int }
fn borrow_pair(p *stats) (*stats, bool) { return p, true }
fn worker(p *stats) { p.count = 9 }
fn check_closures(h ext_holder) {
    s := stats{count: 1}
    p := &s
    f1 := fn() *stats { return p }
    f2 := fn() *stats {
        s2 := stats{count: 2}
        q, ok := borrow_pair(&s2)
        _ = ok
        return q
    }
    f3 := fn() {
        s3 := stats{count: 3}
        q, ok := borrow_pair(&s3)
        _ = ok
        task worker(move q)
        s3.count = 4
    }
    f4 := fn() { h.slot = p }
    _ = f1
    _ = f2
    f3()
    f4()
}"#;
    assert_eq!(check_codes(src_closure), vec!["HYB-E3104", "HYB-E3104", "HYB-E3003", "HYB-E3103"]);
    assert_accept("struct stats { count int }\nfn put_ptr(dst []*stats, p *stats) { dst[0] = p }\nfn borrow_pair(p *stats) (*stats, bool) { return p, true }\nfn check_ok(h ext_holder, ext *stats) {\n    s := stats{count: 1}\n    p := &s\n    f := fn(ext *stats) *stats {\n        s2 := stats{count: 2}\n        q, ok := borrow_pair(&s2)\n        if ok { _ = p.count + q.count }\n        h.slot = ext\n        h.slot, ok = borrow_pair(ext)\n        dst := make([]*stats, 1)\n        put_ptr(dst, ext)\n        f_shadow := fn(p *stats) *stats { return p }\n        f_ext := fn() *stats { return dst[0] }\n        _ = f_shadow(ext)\n        return f_ext()\n    }\n    _ = f(ext)\n}");
}
