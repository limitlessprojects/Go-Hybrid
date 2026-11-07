use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn test_unnamed_fn_type_parameters_width_spelling_issue_3545() {
    let src = r#"
package main

import "fmt"

type f = fn(u8, i32) int

fn ap(g fn(i64) i64, x i64) i64 {
    return g(x)
}

fn main() {
    var g fn(i64) i64 = fn(x i64) i64 { return x + 1 }
    fmt.println(g(1))
}
"#;
    let ast = parse_file("test.hy", src).expect("parses");
    let emitted = emit_go(&ast).expect("emits");

    assert!(emitted.contains("type f = func(uint8, int32) int"), "expected func(uint8, int32) int, got:\n{emitted}");
    assert!(emitted.contains("func(int64) int64"), "expected func(int64) int64, got:\n{emitted}");
    assert!(!emitted.contains("func(i64)"), "unexpected func(i64) in emitted code:\n{emitted}");
    assert!(!emitted.contains("func(u8, i32)"), "unexpected func(u8, i32) in emitted code:\n{emitted}");
}
