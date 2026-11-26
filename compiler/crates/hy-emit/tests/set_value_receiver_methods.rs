use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("test source must parse");
    emit_go(&ast).expect("test source must emit cleanly")
}

#[test]
fn set_contains_and_remove_use_value_receivers_in_prelude() {
    let go = emit(
        "fn make_set() set[int] {\n\
             s := set[int]{}\n\
             s.insert(7)\n\
             return s\n\
         }\n\
         fn main() {\n\
             m := map[string]set[int]{\"k\": make_set()}\n\
             _ = make_set().contains(7)\n\
             _ = m[\"k\"].contains(7)\n\
             m[\"k\"].remove(7)\n\
             _ = set[int]{}.contains(1)\n\
         }\n",
    );
    assert!(
        go.contains("func (s *set[t]) insert(v t) bool {"),
        "insert must keep pointer receiver:\n{go}"
    );
    assert!(
        go.contains("func (s set[t]) contains(v t) bool {"),
        "contains must use value receiver so non-addressable set expressions build:\n{go}"
    );
    assert!(
        go.contains("func (s set[t]) remove(v t) {"),
        "remove must use value receiver so non-addressable set expressions build:\n{go}"
    );
}
