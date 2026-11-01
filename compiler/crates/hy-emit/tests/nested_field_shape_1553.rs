// #1553: nested user-struct field chains must keep foreign member shapes,
// set builtins, and user .pop() methods. The emitters used to read only a
// single `Ident.field` hop through var_struct, so `o.sub.mu.lock()` lost the
// sync.mutex shape (go build died with "has no field or method lock, but
// does have method Lock"), `len(o.sub.seen)` / `delete(o.sub.seen, k)` fell
// through to raw Go builtins on the set[T] wrapper, and a user `.pop()`
// three levels deep was hijacked into `hy_pop(&...)`.
use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("test source must parse");
    emit_go(&ast).expect("test source must emit cleanly")
}

#[test]
fn nested_mutex_atomic_and_set_fields_keep_their_shapes() {
    // The issue reproducer: two hops from the var to each shape-known field.
    let go = emit(
        "struct inner_1553 {\n\
           mu sync.mutex\n\
           hits atomic.int64\n\
           seen set[string]\n\
         }\n\
         struct outer_1553 {\n\
           sub inner_1553\n\
         }\n\
         fn main() {\n\
           var o outer_1553\n\
           o.sub.mu.lock()\n\
           o.sub.hits.add(1)\n\
           fmt.println(len(o.sub.seen))\n\
           delete(o.sub.seen, \"k\")\n\
           o.sub.mu.unlock()\n\
         }\n",
    );
    assert!(
        go.contains("o.sub.mu.Lock()") && go.contains("o.sub.mu.Unlock()"),
        "expected sync.mutex methods through a two-hop chain, got:\n{go}"
    );
    assert!(
        go.contains("o.sub.hits.Add(1)"),
        "expected atomic.int64 Add through a two-hop chain, got:\n{go}"
    );
    assert!(
        go.contains("o.sub.seen.len()"),
        "expected len(o.sub.seen) to lower to o.sub.seen.len(), got:\n{go}"
    );
    assert!(
        go.contains("o.sub.seen.remove(\"k\")"),
        "expected delete(o.sub.seen, \"k\") to lower to .remove, got:\n{go}"
    );
    assert!(
        !go.contains(".lock()") && !go.contains(".unlock()") && !go.contains(".add(1)"),
        "lowercase foreign names leaked:\n{go}"
    );
    assert!(
        !go.contains("len(o.sub.seen)") && !go.contains("delete(o.sub.seen"),
        "raw Go builtins leaked on the nested set field:\n{go}"
    );
}

#[test]
fn four_hop_and_parenthesized_chains_resolve() {
    // Deeper than two hops, plus a parenthesized receiver link.
    let go = emit(
        "struct leaf_1553 {\n\
           mu sync.mutex\n\
           seen set[string]\n\
         }\n\
         struct mid_1553 {\n\
           l leaf_1553\n\
         }\n\
         struct top_1553 {\n\
           m mid_1553\n\
         }\n\
         fn main() {\n\
           var t top_1553\n\
           t.m.l.mu.lock()\n\
           (t.m.l).mu.unlock()\n\
           fmt.println(len(t.m.l.seen))\n\
           delete(t.m.l.seen, \"z\")\n\
         }\n",
    );
    assert!(
        go.contains("t.m.l.mu.Lock()"),
        "expected a three-hop chain to emit Lock, got:\n{go}"
    );
    assert!(
        go.contains("t.m.l.mu.Unlock()"),
        "expected the parenthesized link to resolve and emit peeled, got:\n{go}"
    );
    assert!(
        go.contains("t.m.l.seen.len()") && go.contains("t.m.l.seen.remove(\"z\")"),
        "expected set builtins through the deep chain, got:\n{go}"
    );
    assert!(
        !go.contains("len(t.m.l.seen)") && !go.contains("delete(t.m.l.seen"),
        "raw Go builtins leaked on the deep set field:\n{go}"
    );
}

#[test]
fn address_of_chain_resolves_like_the_bare_var() {
    // `(&o).sub` is the same user struct as `o.sub` — AddrOf must not
    // break the chain walk (the AddrOf arm of user_struct_name_of).
    let go = emit(
        "struct inner_1553b {\n\
           mu sync.mutex\n\
         }\n\
         struct outer_1553b {\n\
           sub inner_1553b\n\
         }\n\
         fn main() {\n\
           var o outer_1553b\n\
           (&o).sub.mu.lock()\n\
         }\n",
    );
    assert!(
        go.contains(".mu.Lock()"),
        "expected Lock through the addr-of chain, got:\n{go}"
    );
    assert!(!go.contains(".lock()"), "lowercase leaked:\n{go}");
}

#[test]
fn nested_user_struct_pop_method_is_not_hijacked() {
    // Reverse protection: a user-defined pop three levels deep stays a
    // method call; only non-user-struct bases lower to hy_pop.
    let go = emit(
        "struct custom_stack_1553 {\n\
           top int\n\
         }\n\
         fn (c *custom_stack_1553) pop() int {\n\
           return c.top\n\
         }\n\
         struct holder_1553 {\n\
           st custom_stack_1553\n\
         }\n\
         struct box_1553 {\n\
           h holder_1553\n\
         }\n\
         fn main() {\n\
           var b box_1553\n\
           v := b.h.st.pop()\n\
           _ = v\n\
         }\n",
    );
    assert!(
        go.contains("v := b.h.st.pop()"),
        "expected the user .pop() method to survive the nested chain, got:\n{go}"
    );
    assert!(
        !go.contains("hy_pop("),
        "nested user-struct .pop() must not emit hy_pop:\n{go}"
    );
}

#[test]
fn aliased_struct_field_hops_still_resolve() {
    // `sub wrap_c` names a `type alias = struct`, not the struct itself;
    // the alias link must unwrap before the shape table is consulted.
    let go = emit(
        "struct leaf_1553c {\n\
           mu sync.mutex\n\
         }\n\
         type wrap_1553c = leaf_1553c\n\
         struct holder_1553c {\n\
           sub wrap_1553c\n\
         }\n\
         fn main() {\n\
           var h holder_1553c\n\
           h.sub.mu.lock()\n\
         }\n",
    );
    assert!(
        go.contains("h.sub.mu.Lock()"),
        "expected the alias hop to unwrap to leaf_1553c's sync.mutex shape, got:\n{go}"
    );
    assert!(!go.contains(".lock()"), "lowercase leaked:\n{go}");
}

#[test]
fn nested_real_slice_field_still_lowers_to_hy_pop() {
    // Counterpart protection: the fix must not stop REAL slice fields from
    // lowering — `w.sb.items` is []int, not a user struct.
    let go = emit(
        "struct stack_box_1553 {\n\
           items []int\n\
         }\n\
         struct wrap_1553 {\n\
           sb stack_box_1553\n\
         }\n\
         fn main() {\n\
           w := wrap_1553{sb: stack_box_1553{items: []int{1, 2}}}\n\
           x := w.sb.items.pop()\n\
           _ = x\n\
         }\n",
    );
    assert!(
        go.contains("hy_pop(&w.sb.items)"),
        "expected a nested real slice .pop() to keep lowering to hy_pop, got:\n{go}"
    );
}
