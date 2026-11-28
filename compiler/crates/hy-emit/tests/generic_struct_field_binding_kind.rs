use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("test source must parse");
    emit_go(&ast).expect("test source must emit cleanly")
}

#[test]
fn generic_struct_field_set_chan_map_lowering() {
    let go = emit(
        "struct box[t any] {\n\
             v t\n\
         }\n\
         struct pair[a any, b any] {\n\
             first a\n\
             second b\n\
         }\n\
         fn use_box(bs box[set[int]]) int {\n\
             return len(bs.v)\n\
         }\n\
         fn main() {\n\
             bs := box[set[int]]{v: set[int]{}}\n\
             bs.v.insert(42)\n\
             _ = len(bs.v)\n\
             delete(bs.v, 42)\n\
             for x in bs.v {\n\
                 _ = x\n\
             }\n\
             bc := box[chan int]{v: make(chan int, 1)}\n\
             for v in bc.v {\n\
                 _ = v\n\
             }\n\
             bm := box[map[string]int]{v: map[string]int{\"a\": 1}}\n\
             for k in bm.v {\n\
                 var s string = k\n\
                 _ = s\n\
             }\n\
             s := bs.v\n\
             _ = len(s)\n\
             _ = use_box(bs)\n\
             p := pair[map[string]int, set[int]]{first: map[string]int{\"k\": 1}, second: set[int]{}}\n\
             _ = len(p.second)\n\
             for k in p.first {\n\
                 _ = k\n\
             }\n\
         }\n",
    );
    assert!(
        go.contains("bs.v.len()"),
        "expected `bs.v.len()` for box[set[int]], got:\n{go}"
    );
    assert!(
        go.contains("bs.v.remove(42)"),
        "expected `bs.v.remove(42)` for box[set[int]], got:\n{go}"
    );
    assert!(
        go.contains("for x := range bs.v.m"),
        "expected `for x := range bs.v.m` for box[set[int]], got:\n{go}"
    );
    assert!(
        go.contains("for v := range bc.v"),
        "expected single-var channel range `for v := range bc.v`, got:\n{go}"
    );
    assert!(
        go.contains("for k := range bm.v"),
        "expected single-var map key range `for k := range bm.v`, got:\n{go}"
    );
    assert!(
        go.contains("s.len()"),
        "expected `s.len()` for alias `s := bs.v`, got:\n{go}"
    );
    assert!(
        go.contains("p.second.len()"),
        "expected `p.second.len()` for multi-arg generic struct pair, got:\n{go}"
    );
    assert!(
        go.contains("for k := range p.first"),
        "expected `for k := range p.first` for multi-arg generic struct pair, got:\n{go}"
    );
}
