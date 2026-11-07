use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn test_logical_op_evaluation_order_before_try_issue_3116() {
    let src = r#"
package main
import "fmt"

struct state { value bool }

fn flip(s *state) (int, error) {
    s.value = false
    return 1, nil
}

fn observe(b bool, n int) { fmt.println(b, n) }

fn main() {
    s := state{value: true}
    observe(s.value && true, flip(&s)?)
}
"#;
    let ast = parse_file("test.hy", src).expect("parses");
    let emitted = emit_go(&ast).expect("emits");

    // s.value && true must be snapshotted into a temp before flip(&s) is hoisted
    assert!(
        emitted.contains(":= s.value && true"),
        "expected s.value && true to be hoisted into a temp before flip(&s), got:\n{emitted}"
    );
}
