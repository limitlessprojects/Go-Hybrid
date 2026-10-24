use hy_emit::emit_go;
use hy_syntax::parse_file;
use std::process::Command;

fn emit_and_run(src: &str, tag: &str) -> (String, String) {
    let ast = parse_file("test.hy", src).expect("parse");
    let go = emit_go(&ast).expect("emit_go");
    let dir = std::env::temp_dir().join(format!("hy_emit_try_loan_{}_{}", tag, std::process::id()));
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
fn try_define_early_error_return_flushes_live_map_slot_loan() {
    let src = "\
import \"errors\"
import \"fmt\"

struct counter {
    n int
}

fn fail_step() (int, error) {
    return 0, errors.new(\"boom\")
}

fn update_then_try(m map[string]counter) (int, error) {
    c := &m[\"k\"]
    c.n = 42
    x := fail_step()?
    c.n = c.n + x
    return c.n, nil
}

fn main() {
    m := map[string]counter{\"k\": counter{n: 10}}
    _, err := update_then_try(m)
    fmt.println(err != nil, m[\"k\"].n)
}
";
    let (_go, stdout) = emit_and_run(src, "try_def");
    assert_eq!(stdout.trim(), "true 42");
}

#[test]
fn return_fallible_call_with_try_flushes_slot_loan_on_both_paths() {
    let src = "\
import \"errors\"
import \"fmt\"

struct counter {
    n int
}

fn mutate_and_maybe_fail(c *counter, should_fail bool) (int, error) {
    c.n = c.n + 7
    if should_fail {
        return 0, errors.new(\"fail\")
    }
    return c.n, nil
}

fn apply(m map[string]counter, should_fail bool) (int, error) {
    c := &m[\"k\"]
    return mutate_and_maybe_fail(c, should_fail)?, nil
}

fn main() {
    m := map[string]counter{\"k\": counter{n: 10}}
    _, err1 := apply(m, true)
    v2, err2 := apply(m, false)
    fmt.println(err1 != nil, err2 == nil, v2, m[\"k\"].n)
}
";
    let (_go, stdout) = emit_and_run(src, "ret_try");
    assert_eq!(stdout.trim(), "true true 24 24");
}
