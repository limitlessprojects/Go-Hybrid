use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("test source must parse");
    emit_go(&ast).expect("test source must emit cleanly")
}

#[test]
fn nested_user_struct_fields_resolve_foreign_shapes_sets_and_user_pop() {
    let go = emit(
        "import \"sync\"\n\
         import \"sync/atomic\"\n\
         struct custom_stack {\n\
           top int\n\
         }\n\
         fn (c *custom_stack) pop() int {\n\
           return c.top\n\
         }\n\
         struct inner {\n\
           mu sync.mutex\n\
           hits atomic.int64\n\
           seen set[string]\n\
           st custom_stack\n\
           buf []int\n\
         }\n\
         struct outer {\n\
           sub inner\n\
         }\n\
         fn main() {\n\
           var o outer\n\
           o.sub.mu.lock()\n\
           o.sub.hits.add(1)\n\
           n := len(o.sub.seen)\n\
           delete(o.sub.seen, \"k\")\n\
           p := o.sub.st.pop()\n\
           q := o.sub.buf.pop()\n\
           o.sub.mu.unlock()\n\
           _ = n + p + q\n\
         }\n",
    );
    assert!(go.contains("o.sub.mu.Lock()"), "{go}");
    assert!(go.contains("o.sub.hits.Add(1)"), "{go}");
    assert!(go.contains("n := o.sub.seen.len()"), "{go}");
    assert!(go.contains("o.sub.seen.remove(\"k\")"), "{go}");
    assert!(go.contains("p := o.sub.st.pop()"), "{go}");
    assert!(go.contains("q := hy_pop(&o.sub.buf)"), "{go}");
    assert!(go.contains("o.sub.mu.Unlock()"), "{go}");
}
