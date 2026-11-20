//! Soundness & Row-13 buildability regression suite for O8 slot-loan closure
//! alias tracking (`closure_captured_loans`, `emit_assign_in_seq`, `slot_loan_alias_rhs`).
use hy_emit::emit_go;
use hy_syntax::parse_file;
use std::process::Command;

const HEAD: &str = "package main\nimport \"fmt\"\nstruct item { val int }\n";

fn emit(src: &str) -> String {
    emit_go(&parse_file("slot_loan_closure_alias_soundness.hy", src).expect("parse")).expect("emit")
}

fn run(tag: &str, body: &str) -> String {
    let go_src = emit(&format!("{HEAD}{body}"));
    let go_file = std::env::temp_dir().join(format!("hy_loan_alias_{}_{}.go", std::process::id(), tag));
    std::fs::write(&go_file, &go_src).expect("write go");
    let out = Command::new("go").args(["run", go_file.to_str().unwrap()]).output().expect("go run");
    let _ = std::fs::remove_file(&go_file);
    assert!(out.status.success(), "{}\n{go_src}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn non_closure_read_flushes_loan_before_task_move() {
    let body = r#"fn worker(m map[string]item, ready chan bool, done chan int) {
    _ = <-ready
    for i in 0..200 { m["b"] = item{val: i} }
    done <- 1
}
fn main() {
    for round in 0..40 {
        m := map[string]item{"a": item{val: round}}
        p := &m["a"]
        p.val = 42
        snap := p.val + 0
        ready := make(chan bool, 1)
        done := make(chan int, 1)
        task worker(move m, ready, done)
        ready <- true
        _ = snap
        _ = <-done
    }
    fmt.println("survived")
}
"#;
    let go = emit(&format!("{HEAD}{body}"));
    assert!(go.find(r#"m["a"] = p"#).unwrap() < go.find("go worker(").unwrap(), "{go}");
    assert_eq!(run("post_move_no_race", body), "survived");
}

#[test]
fn scalar_and_value_returning_closure_reads_do_not_clobber_later_map_write() {
    let scalar = "fn main() {\n    m := map[string]item{\"a\": item{val: 1}}\n    p := &m[\"a\"]\n    p.val = 42\n    snap := p.val + 0\n    m[\"a\"] = item{val: 99}\n    _ = snap\n    fmt.println(m[\"a\"].val)\n}\n";
    assert_eq!(run("scalar_clobber", scalar), "99");
    let getter = "fn main() {\n    m := map[string]item{\"a\": item{val: 1}}\n    p := &m[\"a\"]\n    p.val = 42\n    get := fn() int { return p.val }\n    n := get()\n    m[\"a\"] = item{val: 99}\n    _ = n\n    fmt.println(m[\"a\"].val)\n}\n";
    assert_eq!(run("getter_clobber", getter), "99");
}

#[test]
fn struct_copy_from_loan_then_rebind_stays_buildable() {
    let body = "fn main() {\n    m := map[string]item{\"a\": item{val: 1}, \"b\": item{val: 2}}\n    p := &m[\"a\"]\n    first := item{val: p.val}\n    second := first\n    _ = p.val\n    second = &m[\"b\"]\n    fmt.println(second.val)\n}\n";
    assert_eq!(run("struct_rebind_buildable", body), "2");
}

#[test]
fn assigned_second_order_and_projected_closures_preserve_loan_writes() {
    let second_order = "fn main() {\n    m := map[string]item{\"a\": item{val: 1}}\n    var f fn()\n    p := &m[\"a\"]\n    mk := fn() fn() { return fn() { p.val = 42 } }\n    f = mk()\n    f()\n    fmt.println(m[\"a\"].val)\n}\n";
    assert_eq!(run("second_order_assign", second_order), "42");
    let projected = "struct runner { run fn() }\nfn main() {\n    m := map[string]item{\"a\": item{val: 1}}\n    var r runner\n    p := &m[\"a\"]\n    r.run = fn() { p.val = 42 }\n    r.run()\n    fmt.println(m[\"a\"].val)\n}\n";
    assert_eq!(run("projected_assign", projected), "42");
}
