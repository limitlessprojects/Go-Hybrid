//! #3209: a slot loan captured by a deferred closure must be stored back
//! after the deferred call runs (function exit), not only at the end of the
//! body, or the deferred write lands on a dead copy.
use hy_emit::emit_go;
use hy_syntax::parse_file;
use std::process::Command;

fn run(name: &str, src: &str) -> String {
    let go_src = emit_go(&parse_file("test.hy", src).expect("parse")).expect("emit");
    let go_file = std::env::temp_dir().join(format!("hy_3209_{name}.go"));
    std::fs::write(&go_file, &go_src).expect("write go");
    let out = Command::new("go")
        .args(["run", go_file.to_str().unwrap()])
        .output()
        .expect("go run");
    let _ = std::fs::remove_file(&go_file);
    assert!(
        out.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&out.stderr),
        go_src
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

const HEAD: &str = "package main\nimport \"fmt\"\nstruct cell { n int }\n";

#[test]
fn deferred_closure_write_lands_slice() {
    let src = format!("{HEAD}fn main() {{\n    xs := []cell{{cell{{n: 1}}}}\n    run(xs)\n    fmt.println(xs[0].n)\n}}\nfn run(xs []cell) {{\n    p := &xs[0]\n    defer fn() {{ p.n = 7 }}()\n}}\n");
    assert_eq!(run("slice", &src), "7");
}

#[test]
fn deferred_closure_write_lands_map_slot() {
    let src = format!("{HEAD}fn main() {{\n    m := map[string]cell{{\"a\": cell{{n: 1}}}}\n    run(m)\n    fmt.println(m[\"a\"].n)\n}}\nfn run(m map[string]cell) {{\n    p := &m[\"a\"]\n    defer fn() {{ p.n = 7 }}()\n}}\n");
    assert_eq!(run("map", &src), "7");
}

#[test]
fn deferred_write_runs_after_direct_write() {
    let src = format!("{HEAD}fn main() {{\n    xs := []cell{{cell{{n: 1}}}}\n    run(xs)\n    fmt.println(xs[0].n)\n}}\nfn run(xs []cell) {{\n    p := &xs[0]\n    defer fn() {{ p.n = 7 }}()\n    p.n = 3\n}}\n");
    assert_eq!(run("after_direct", &src), "7");
}

#[test]
fn deferred_closure_sees_direct_write_and_early_return() {
    // The deferred closure reads the loan after a direct write and adds to
    // it; an early `return` path must still persist the deferred write.
    let src = format!("{HEAD}fn main() {{\n    xs := []cell{{cell{{n: 1}}}}\n    run(xs, true)\n    fmt.println(xs[0].n)\n    run(xs, false)\n    fmt.println(xs[0].n)\n}}\nfn run(xs []cell, early bool) {{\n    p := &xs[0]\n    defer fn() {{ p.n += 100 }}()\n    p.n += 1\n    if early {{\n        return\n    }}\n    p.n += 10\n}}\n");
    assert_eq!(run("early_return", &src), "102\n213");
}

#[test]
fn defer_without_loan_is_unchanged() {
    let src = format!("{HEAD}fn main() {{\n    xs := []cell{{cell{{n: 1}}}}\n    p := &xs[0]\n    p.n = 2\n    defer fmt.println(\"bye\")\n    fmt.println(xs[0].n)\n}}\n");
    let go = emit_go(&parse_file("test.hy", &src).expect("parse")).expect("emit");
    assert!(
        !go.contains("defer func() {"),
        "unexpected deferred store-back:\n{go}"
    );
    assert_eq!(run("no_loan", &src), "2\nbye");
}
