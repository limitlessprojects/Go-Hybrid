//! #1927: checker-planned clipping, through the actual CLI handoff.
use std::{fs, path::PathBuf, process::Command};

struct Scratch(PathBuf);
impl Scratch {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!("hyb-clip-{tag}-{}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn build_and_run(tag: &str, source: &str, expected: &str) {
    let scratch = Scratch::new(tag);
    let source_path = scratch.0.join("main.hy");
    let binary = scratch.0.join("program");
    fs::write(&source_path, source).unwrap();
    let build = Command::new(env!("CARGO_BIN_EXE_hyb"))
        .args(["build"])
        .arg(&source_path)
        .arg("-o")
        .arg(&binary)
        .current_dir(&scratch.0)
        .output()
        .expect("run hyb build");
    assert!(build.status.success(), "build failed:\n{}\n{}",
            String::from_utf8_lossy(&build.stdout), String::from_utf8_lossy(&build.stderr));
    let run = Command::new(binary).output().expect("run compiled program");
    assert!(run.status.success(), "program failed: {}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(String::from_utf8(run.stdout).unwrap(), expected);
}

#[test]
fn clips_all_two_index_forms_and_append_cannot_overwrite_tail() {
    build_and_run("forms", r#"
fn main() {
    s := make([]int, 4, 8)
    s[2] = 77
    a := s[1:2]
    b := s[1:]
    c := s[:2]
    d := s[:]
    e := s[2:2]
    var empty []int
    f := empty[:]
    fmt.println(len(a), cap(a), len(b), cap(b), len(c), cap(c), len(d), cap(d))
    fmt.println(len(e), cap(e), len(f), cap(f))
    a = append(a, 99)
    fmt.println(a[1], s[2])
}
"#, "1 1 3 3 2 2 4 4\n0 0 0 0\n99 77\n");
}

#[test]
fn arrays_and_array_pointers_keep_the_original_storage() {
    build_and_run("arrays", r#"
struct holder { data [4]int }
fn main() {
    var a [4]int
    p := &a
    s := a[1:2]
    t := p[2:3]
    s[0] = 11
    t[0] = 22
    var h holder
    u := h.data[:2]
    u[1] = 33
    fmt.println(len(s), cap(s), len(t), cap(t), len(u), cap(u))
    fmt.println(a[1], a[2], h.data[1])
}
"#, "1 1 1 1 2 2\n11 22 33\n");
}

#[test]
fn base_and_bounds_run_once_in_source_order() {
    build_and_run("order", r#"
fn source(state []int) []int {
    state[0] = state[0] * 10 + 1
    return []int{10, 20, 30, 40}
}
fn bound(state []int, digit int, value int) int {
    state[0] = state[0] * 10 + digit
    return value
}
fn main() {
    state := []int{0}
    s := source(state)[bound(state, 2, 1):bound(state, 3, 3)]
    fmt.println(state[0], len(s), cap(s), s[0])
    state[0] = 0
    t := source(state)[bound(state, 2, 1):]
    fmt.println(state[0], len(t), cap(t))
}
"#, "123 2 2 20\n12 3 3\n");
}

#[test]
fn explicit_full_slice_preserves_spare_capacity_and_bound_order() {
    build_and_run("full", r#"
fn source(state []int) []int {
    state[0] = state[0] * 10 + 1
    return []int{10, 20, 30, 40}
}
fn bound(state []int, digit int, value int) int {
    state[0] = state[0] * 10 + digit
    return value
}
fn main() {
    state := []int{0}
    s := source(state)[bound(state, 2, 1):bound(state, 3, 2):bound(state, 4, 4)]
    fmt.println(state[0], len(s), cap(s))
    a := []int{1, 2, 3, 4}
    t := a[:2:4]
    t = append(t, 99)
    fmt.println(len(t), cap(t), a[2])
    u := t[1:2]
    fmt.println(len(u), cap(u))
}
"#, "1234 1 3\n3 4 99\n1 1\n");
}

#[test]
fn nested_views_closures_and_strings_keep_their_types() {
    build_and_run("nested", r#"
fn main() {
    s := make([]int, 4, 8)
    view := fn(xs []int) []int { return xs[1:3] }
    a := view(s)[:1]
    text := "abcd"
    b := text[1:3]
    c := text[:2]
    d := text[2:]
    fmt.println(len(a), cap(a), b, c, d, text[:])
}
"#, "1 1 bc ab cd abcd\n");
}

#[test]
fn try_in_max_preserves_order_of_earlier_calls() {
    build_and_run("try-max", r#"
fn source(state []int) []int {
    state[0] = state[0] * 10 + 1
    return []int{10, 20, 30, 40}
}
fn bound(state []int, digit int, value int) int {
    state[0] = state[0] * 10 + digit
    return value
}
fn maximum(state []int) (int, error) {
    state[0] = state[0] * 10 + 4
    return 4, nil
}
fn main() {
    state := []int{0}
    s := source(state)[bound(state, 2, 1):bound(state, 3, 2):maximum(state)?]
    fmt.println(state[0], len(s), cap(s))
}
"#, "1234 1 3\n");
}

#[test]
fn generate_threads_the_plan_into_an_importable_package() {
    let scratch = Scratch::new("generate");
    fs::write(scratch.0.join("go.mod"), "module cliptest\n\ngo 1.26.9\n").unwrap();
    fs::write(scratch.0.join("package.go"), "package cliptest\n").unwrap();
    let source = scratch.0.join("views.hy");
    fs::write(&source, "pub fn view(s []int) []int { return s[1:2] }\n").unwrap();
    let generated = Command::new(env!("CARGO_BIN_EXE_hyb"))
        .arg("generate").arg(&source).output().expect("run generate");
    assert!(generated.status.success(), "{}\n{}", String::from_utf8_lossy(&generated.stdout),
            String::from_utf8_lossy(&generated.stderr));
    fs::write(scratch.0.join("views_test.go"), r#"package cliptest
import "testing"
func TestCapacity(t *testing.T) {
    v := View(make([]int, 4, 8))
    if len(v) != 1 || cap(v) != 1 { t.Fatalf("len=%d cap=%d", len(v), cap(v)) }
}
"#).unwrap();
    let test = Command::new(std::env::var("HYB_GO").unwrap_or_else(|_| "go".into()))
        .args(["test", "."]).current_dir(&scratch.0).output().expect("go test generated package");
    assert!(test.status.success(), "{}\n{}", String::from_utf8_lossy(&test.stdout),
            String::from_utf8_lossy(&test.stderr));
}

#[test]
fn array_base_with_side_effect_and_try_bound_is_not_copied() {
    build_and_run("array-try", r#"
struct holder { data [4]int }
fn source(h *holder, state []int) *holder {
    state[0] = state[0] * 10 + 1
    return h
}
fn high(state []int) (int, error) {
    state[0] = state[0] * 10 + 2
    return 1, nil
}
fn maximum(state []int) (int, error) {
    state[0] = state[0] * 10 + 2
    return 4, nil
}
fn main() {
    var h holder
    state := []int{0}
    a := source(&h, state).data[:high(state)?]
    a[0] = 77
    fmt.println(state[0], cap(a), h.data[0])
    state[0] = 0
    b := source(&h, state).data[:1:maximum(state)?]
    b[0] = 88
    fmt.println(state[0], cap(b), h.data[0])
}
"#, "12 1 77\n12 4 88\n");
}

#[test]
fn helper_symbol_does_not_capture_a_source_parameter() {
    // main.hy in build_and_run determines the initial helper basename.
    build_and_run("hygiene", r#"
fn view(hy_clip_6d61696e2e6879 int, s []int) []int { return s[1:2] }
fn main() {
    s := view(7, make([]int, 4, 8))
    fmt.println(len(s), cap(s))
}
"#, "1 1\n");
}

#[test]
fn multiple_generated_files_can_share_one_go_package() {
    let scratch = Scratch::new("multiple");
    fs::write(scratch.0.join("go.mod"), "module cliptest\n\ngo 1.26.9\n").unwrap();
    fs::write(scratch.0.join("package.go"), "package cliptest\n").unwrap();
    fs::write(scratch.0.join("one.hy"), r#"
fn hy_clip(s []int) []int { return s }
fn hy_clip_6f6e652e6879(n int) int { return n + 1 }
pub fn one(s []int) []int { return hy_clip(s)[:1] }
"#).unwrap();
    fs::write(scratch.0.join("two.hy"), "pub fn two(s []int) []int { return s[1:2] }\n").unwrap();
    let generated = Command::new(env!("CARGO_BIN_EXE_hyb"))
        .arg("generate").arg(&scratch.0).output().expect("run generate");
    assert!(generated.status.success(), "{}\n{}", String::from_utf8_lossy(&generated.stdout),
            String::from_utf8_lossy(&generated.stderr));
    let emitted = fs::read_to_string(scratch.0.join("one.hy.go")).unwrap();
    let helper = emitted.lines().find(|line| line.starts_with("func hy_clip_") && line.contains("["))
        .expect("clipping prelude").trim_start_matches("func ").split('[').next().unwrap();
    fs::write(scratch.0.join("views_test.go"), format!(r#"package cliptest
import "testing"
type named []int
func (s named) marker() int {{ return 42 }}
func TestCapacity(t *testing.T) {{
    for _, f := range []func([]int) []int{{One, Two}} {{
        v := f(make([]int, 4, 8))
        if len(v) != 1 || cap(v) != 1 {{ t.Fatalf("len=%d cap=%d", len(v), cap(v)) }}
    }}
    if cap(hy_clip(make([]int, 4, 8))) != 8 {{ t.Fatal("user hy_clip was changed") }}
    if hy_clip_6f6e652e6879(41) != 42 {{ t.Fatal("user helper-like symbol was changed") }}
    v := {helper}(named(make([]int, 4, 8))[1:2])
    if len(v) != 1 || cap(v) != 1 || v.marker() != 42 {{
        t.Fatalf("named slice len=%d cap=%d", len(v), cap(v))
    }}
}}
"#)).unwrap();
    let test = Command::new(std::env::var("HYB_GO").unwrap_or_else(|_| "go".into()))
        .args(["test", "."]).current_dir(&scratch.0).output().expect("go test generated package");
    assert!(test.status.success(), "{}\n{}", String::from_utf8_lossy(&test.stdout),
            String::from_utf8_lossy(&test.stderr));
    let check = Command::new(env!("CARGO_BIN_EXE_hyb"))
        .args(["generate", "--check", "."]).current_dir(&scratch.0)
        .output().expect("check relative filename stability");
    assert!(check.status.success(), "{}\n{}", String::from_utf8_lossy(&check.stdout),
            String::from_utf8_lossy(&check.stderr));
}
