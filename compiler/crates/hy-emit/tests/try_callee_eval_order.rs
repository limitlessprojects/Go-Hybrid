use hy_emit::emit_go;
use hy_syntax::parse_file;
use std::process::Command;

#[test]
fn indexed_and_parenthesized_callees_evaluate_before_failing_try_arg() {
    let src = r#"package main
import "errors"
import "fmt"
struct counter { n int }
fn bump(c *counter) int { c.n = c.n + 1
 return 0 }
fn nop(x int) {}
fn fail() (int, error) { return 0, errors.new("boom") }
fn run(c *counter) error {
    fns := []fn(a int){nop}
    fns[bump(c)](fail()?)
    return nil
}
fn main() {
    c := counter{n: 0}
    _ = run(&c)
    fmt.println(c.n)
}
"#;
    let go_src = emit_go(&parse_file("test.hy", src).expect("parse")).expect("emit");
    let go_file = std::env::temp_dir().join("hy_try_callee_eval_order_prog.go");
    std::fs::write(&go_file, go_src).expect("write go");
    let out = Command::new("go").args(["run", go_file.to_str().unwrap()]).output().expect("go run");
    let _ = std::fs::remove_file(&go_file);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "1");
}
