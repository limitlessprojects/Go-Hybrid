use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn indexed_container_elements_keep_set_and_map_lowering() {
    let src = r#"
package main

import "fmt"

fn main() {
    ss := []set[int]{set[int]{}}
    fmt.println(len(ss[0]))
    for v in ss[0] {
        fmt.println(v)
    }
    delete(ss[0], 1)
    m := map[string]set[int]{}
    x := m["a"]
    fmt.println(len(x))
    ms := []map[string]int{map[string]int{"a": 1}}
    for k in ms[0] {
        fmt.println(k)
    }
    inner := ms[0]
    for k in inner {
        fmt.println(k)
    }
}
"#;
    let ast = parse_file("test.hy", src).expect("parses");
    let emitted = emit_go(&ast).expect("emits");
    for want in [
        "ss[0].len()",
        "range ss[0].m",
        "ss[0].remove(1)",
        "x.len()",
        "for k := range ms[0]",
        "for k := range inner",
    ] {
        assert!(emitted.contains(want), "expected `{want}` in:\n{emitted}");
    }
    for bad in [
        "len(ss[0])",
        "for _, k := range ms[0]",
        "for _, k := range inner",
    ] {
        assert!(!emitted.contains(bad), "unexpected `{bad}` in:\n{emitted}");
    }
}
