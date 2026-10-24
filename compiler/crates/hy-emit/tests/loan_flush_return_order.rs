use hy_emit::emit_go;
use hy_syntax::parse_file;
use std::process::Command;

fn emit_and_run(src: &str, tag: &str) -> (String, String) {
    let ast = parse_file("test.hy", src).expect("parse");
    let go = emit_go(&ast).expect("emit_go");
    let dir = std::env::temp_dir().join(format!("hy_emit_ret_loan_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let go_path = dir.join("main.go");
    std::fs::write(&go_path, &go).expect("write go");
    let out = Command::new("go")
        .arg("run")
        .arg(&go_path)
        .output()
        .expect("go run");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        out.status.success(),
        "go run failed:\n{}\n--- go ---\n{}",
        String::from_utf8_lossy(&out.stderr),
        go
    );
    (go, String::from_utf8_lossy(&out.stdout).into_owned())
}

#[test]
fn return_method_call_on_map_slot_loan_flushes_post_mutation_value() {
    let src = "\
import \"fmt\"

struct counter {
    n int
}

fn (c *counter) next() int {
    c.n = c.n + 1
    return c.n
}

fn bump(m map[string]counter) int {
    c := &m[\"k\"]
    return c.next()
}

fn main() {
    m := map[string]counter{\"k\": counter{n: 10}}
    got := bump(m)
    fmt.println(got, m[\"k\"].n)
}
";
    let (_go, stdout) = emit_and_run(src, "method");
    assert_eq!(stdout.trim(), "11 11");
}

#[test]
fn return_multi_value_call_mutating_slot_loan_flushes_after_call() {
    let src = "\
import \"fmt\"

struct counter {
    n int
}

fn step_pair(c *counter) (int, int) {
    before := c.n
    c.n = c.n + 5
    return before, c.n
}

fn run_pair(m map[string]counter) (int, int) {
    c := &m[\"k\"]
    return step_pair(c)
}

fn main() {
    m := map[string]counter{\"k\": counter{n: 20}}
    a, b := run_pair(m)
    fmt.println(a, b, m[\"k\"].n)
}
";
    let (_go, stdout) = emit_and_run(src, "pair");
    assert_eq!(stdout.trim(), "20 25 25");
}

#[test]
fn return_interpolated_string_with_slot_loan_mutation_call() {
    let src = "\
import \"fmt\"

struct counter {
    n int
}

fn (c *counter) next() int {
    c.n = c.n + 1
    return c.n
}

fn bump_interp(m map[string]counter) string {
    c := &m[\"k\"]
    return \"v={c.next()}\"
}

fn main() {
    m := map[string]counter{\"k\": counter{n: 10}}
    got := bump_interp(m)
    fmt.println(got, m[\"k\"].n)
}
";
    let (_go, stdout) = emit_and_run(src, "interp");
    assert_eq!(stdout.trim(), "v=11 11");
}
