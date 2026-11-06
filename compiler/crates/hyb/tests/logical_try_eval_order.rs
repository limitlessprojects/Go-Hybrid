//! E3b must preserve Go's ordering of binary logical operations before
//! later calls, even when the logical operands contain no calls themselves.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "hyb-logical-try-{tag}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        assert_eq!(self.0.parent(), Some(std::env::temp_dir().as_path()));
        let _ = fs::remove_dir_all(&self.0);
    }
}

const PRELUDE: &str = r#"package main
import "fmt"
struct state { value bool
 touched int }
fn flip(s *state, value bool) (int, error) {
    s.value = value
    return 1, nil
}
fn flip_bool(s *state, value bool) (bool, error) {
    s.value = value
    return value, nil
}
fn observe(value bool, n int) { fmt.println(value, n) }
"#;

fn check_build_run(tag: &str, rest: &str, expected: &str) {
    let dir = Scratch::new(tag);
    let source = dir.0.join("logical.hy");
    fs::write(&source, format!("{PRELUDE}\n{rest}")).unwrap();
    let check = Command::new(env!("CARGO_BIN_EXE_hyb"))
        .args(["check", "--json"])
        .arg(&source)
        .current_dir(&dir.0)
        .output()
        .expect("run checker");
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stdout)
    );
    let report: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    assert_eq!(report["summary"]["verdict"], "accept");
    assert_eq!(report["summary"]["errors"], 0);

    let binary = dir
        .0
        .join(format!("logical{}", std::env::consts::EXE_SUFFIX));
    let build = Command::new(env!("CARGO_BIN_EXE_hyb"))
        .arg("build")
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .current_dir(&dir.0)
        .output()
        .expect("build accepted program");
    assert!(
        build.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(&binary)
        .output()
        .expect("run generated program");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8(run.stdout).unwrap(), expected);
}

#[test]
fn logical_call_arguments_are_evaluated_before_later_try_calls() {
    check_build_run(
        "args",
        r#"
fn main() {
    s := state{value: true}
    observe(s.value && true, flip(&s, false)?)
    s.value = false
    observe(s.value || false, flip(&s, true)?)
    s.value = true
    observe(((s.value && true) || false), flip(&s, false)?)
    s.value = false
    observe(!(!(s.value || false)), flip(&s, true)?)
}
"#,
        "true 1\nfalse 1\ntrue 1\nfalse 1\n",
    );
}

#[test]
fn logical_values_in_composite_literals_keep_their_evaluation_order() {
    check_build_run(
        "composites",
        r#"
struct result { before bool
 after int }
fn main() {
    s := state{value: true}
    r := result{before: s.value && true, after: flip(&s, false)?}
    fmt.println(r.before, r.after)
    s.value = true
    values := []bool{s.value && true, flip_bool(&s, false)?}
    fmt.println(values[0], values[1])
    s.value = false
    values_by_key := map[string]bool{"before": s.value || false, "after": flip_bool(&s, true)?}
    fmt.println(values_by_key["before"], values_by_key["after"])
}
"#,
        "true 1\ntrue false\nfalse true\n",
    );
}

#[test]
fn logical_parallel_bindings_and_assignments_precede_later_try_calls() {
    check_build_run(
        "assign",
        r#"
fn main() {
    s := state{value: true}
    before, after := s.value && true, flip(&s, false)?
    fmt.println(before, after)
    before, after = s.value || false, flip(&s, true)?
    fmt.println(before, after)
}
"#,
        "true 1\nfalse 1\n",
    );
}

#[test]
fn logical_return_values_precede_later_try_calls() {
    check_build_run(
        "return",
        r#"
fn pair(s *state) (bool, error) {
    return (s.value && true) == flip_bool(s, false)?, nil
}
fn main() {
    s := state{value: true}
    value, err := pair(&s)
    fmt.println(value, err)
}
"#,
        "false <nil>\n",
    );
}

#[test]
fn deferred_arguments_capture_the_logical_value_at_registration() {
    check_build_run(
        "defer",
        r#"
fn deferred(s *state) error {
    defer observe(s.value && true, flip(s, false)?)
    fmt.println(s.value)
    return nil
}
fn main() {
    s := state{value: true}
    err := deferred(&s)
    fmt.println(err)
}
"#,
        "false\ntrue 1\n<nil>\n",
    );
}

#[test]
fn nested_logical_values_in_binary_and_interpolated_expressions_are_ordered() {
    check_build_run(
        "nested",
        r#"
fn main() {
    s := state{value: true}
    fmt.println((s.value && true) == flip_bool(&s, false)?)
    s.value = true
    fmt.println("{s.value && true} {flip(&s, false)?}")
}
"#,
        "false\ntrue 1\n",
    );
}

#[test]
fn error_propagation_still_skips_the_call_and_returns_the_error() {
    check_build_run(
        "error",
        r#"
fn fail(s *state) (int, error) {
    s.value = false
    return 0, errors.new("boom")
}
fn attempt(s *state) error {
    observe(s.value && true, fail(s)?)
    fmt.println("unreachable")
    return nil
}
fn main() {
    s := state{value: true}
    err := attempt(&s)
    fmt.println(err, s.value)
}
"#,
        "boom false\n",
    );
}

#[test]
fn short_circuiting_plain_calls_and_closure_bodies_remain_lazy() {
    check_build_run(
        "controls",
        r#"
fn touch(s *state) bool {
    s.touched = s.touched + 1
    return true
}
fn flip_plain(s *state) int {
    s.value = false
    return 1
}
fn callback(f fn() bool, n int) { fmt.println(f(), n) }
fn main() {
    s := state{value: true}
    observe(false && touch(&s), flip(&s, false)?)
    observe(true || touch(&s), flip(&s, false)?)
    fmt.println(s.touched)
    s.value = true
    observe(s.value && true, flip_plain(&s))
    s.value = true
    callback(fn() bool { return s.value && true }, flip(&s, false)?)
}
"#,
        "false 1\ntrue 1\n0\ntrue 1\nfalse 1\n",
    );
}
