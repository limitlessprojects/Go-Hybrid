use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn test_single_var_map_range_emit_issue_3422() {
    let src = r#"
package main

import "fmt"

type map_alias = map[int]string

struct holder {
    m map[string]int
}

fn main() {
    var a map_alias = map_alias{}
    for k in a {
        fmt.println(k)
    }

    h := holder{m: map[string]int{}}
    for k in h.m {
        fmt.println(k)
    }
}
"#;
    let ast = parse_file("test.hy", src).expect("parses");
    let emitted = emit_go(&ast).expect("emits");

    assert!(
        emitted.contains("for k := range a"),
        "expected single-var range over map alias to emit `for k := range a`, got:\n{emitted}"
    );
    assert!(
        emitted.contains("for k := range h.m"),
        "expected single-var range over struct field map to emit `for k := range h.m`, got:\n{emitted}"
    );
    assert!(
        !emitted.contains("for _, k := range a"),
        "unexpected value-binding `for _, k := range a` in emitted code:\n{emitted}"
    );
}
