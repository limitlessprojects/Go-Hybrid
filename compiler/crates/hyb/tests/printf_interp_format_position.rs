//! printf-family calls splice an interpolated FORMAT argument into the call
//! instead of pre-rendering it with `fmt.Sprintf`; an interpolated string in
//! any other argument position stays an ordinary string value.
//!
//! Before the fix, `fmt.printf("{name} n=%d\n", n)` lowered to
//! `fmt.Printf(fmt.Sprintf("%v n=%%d\n", name), n)`: a `%` inside the
//! interpolated value became a verb of the outer call (`name := "100%d"`
//! printed `1005 n=%!d(MISSING)`), and `fmt.printf("[%s]\n", "{a}")` was
//! spliced as if `"{a}"` were the format (`[%v]` plus `%!(EXTRA ...)`).

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
            "hyb-printf-interp-{tag}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Check (must accept), build, run; return stdout and the emitted Go.
fn build_run(tag: &str, program: &str) -> (String, String) {
    let dir = Scratch::new(tag);
    let source = dir.0.join("prog.hy");
    fs::write(&source, program).unwrap();

    let check = Command::new(env!("CARGO_BIN_EXE_hyb"))
        .args(["check", "--json"])
        .arg(&source)
        .current_dir(&dir.0)
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "checker failed: {}{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    assert_eq!(report["summary"]["verdict"], "accept");

    let binary = dir.0.join(format!("prog{}", std::env::consts::EXE_SUFFIX));
    let build = Command::new(env!("CARGO_BIN_EXE_hyb"))
        .arg("build")
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .current_dir(&dir.0)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "build failed: {}{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    let emitted = Command::new(env!("CARGO_BIN_EXE_hyb"))
        .arg("build")
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .arg("--emit-go")
        .current_dir(&dir.0)
        .output()
        .unwrap();
    assert!(emitted.status.success());
    let go = fs::read_to_string(
        dir.0
            .join(format!("prog{}.go", std::env::consts::EXE_SUFFIX)),
    )
    .unwrap_or_default();

    let run = Command::new(&binary).output().unwrap();
    assert!(
        run.status.success(),
        "program failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    (String::from_utf8(run.stdout).unwrap(), go)
}

#[test]
fn percent_in_interpolated_value_is_not_a_verb_when_explicit_args_follow() {
    let (out, go) = build_run(
        "explicit-args",
        r#"import "os"
fn main() {
    name := "100%d"
    n := 5
    fmt.printf("{name} n=%d\n", n)
    s := fmt.sprintf("{name}|%-4d|%5.2f|%x", n, 3.14159, 255)
    fmt.println(s)
    fmt.fprintf(os.stdout, "%*d|{name}\n", 4, n)
    fmt.printf("lit 100%% {name} %v\n", true)
}
"#,
    );
    assert_eq!(
        out,
        "100%d n=5\n100%d|5   | 3.14|ff\n   5|100%d\nlit 100% 100%d true\n"
    );
    assert!(
        !go.contains("Sprintf(fmt.Sprintf") && !go.contains("Printf(fmt.Sprintf"),
        "the format must not be pre-rendered:\n{go}"
    );
}

#[test]
fn percent_in_interpolated_value_is_not_a_verb_in_format_only_calls() {
    let (out, _) = build_run(
        "format-only",
        r#"fn main() {
    p := "50%"
    path := "/tmp/a%20b"
    fmt.println(fmt.sprintf("rate: {p}"))
    fmt.println(fmt.errorf("open {path}: failed"))
    fmt.printf("rate: {p}\n")
}
"#,
    );
    assert_eq!(out, "rate: 50%\nopen /tmp/a%20b: failed\nrate: 50%\n");
}

#[test]
fn interpolated_value_argument_is_not_spliced_as_the_format() {
    let (out, _) = build_run(
        "value-arg",
        r#"import "os"
fn main() {
    a := 3
    b := "x"
    fmt.printf("[%s]\n", "{a}-{b}")
    fmt.fprintf(os.stdout, "[%-8s]\n", "{b}{a}")
    fmt.println(fmt.sprintf("<%s>", "{a}"))
}
"#,
    );
    assert_eq!(out, "[3-x]\n[x3      ]\n<3>\n");
}

#[test]
fn errorf_keeps_wrapping_with_an_interpolated_format() {
    let (out, _) = build_run(
        "errorf-wrap",
        r#"fn main() {
    p := "50%"
    base := errors.new("base")
    e := fmt.errorf("ctx {p}: %w", base)
    fmt.println(e, errors.is(e, base))
}
"#,
    );
    assert_eq!(out, "ctx 50%: base true\n");
}

#[test]
fn log_printf_family_splices_the_interpolated_format() {
    let (out, _) = build_run(
        "log",
        r#"import "log"
import "os"
fn main() {
    p := "50%"
    n := 5
    l := log.new(os.stdout, "", 0)
    l.printf("logger {p}")
    l.printf("logger {p} %d", n)
    log.set_output(os.stdout)
    log.set_flags(0)
    log.printf("pkg {p}")
    log.printf("pkg {p} %s", "x")
}
"#,
    );
    assert_eq!(out, "logger 50%\nlogger 50% 5\npkg 50%\npkg 50% x\n");
}

#[test]
fn spliced_arguments_keep_source_evaluation_order() {
    let (out, _) = build_run(
        "order",
        r#"fn main() {
    var order []string
    next := fn(tag string) int {
        order = append(order, tag)
        return len(order)
    }
    fmt.printf("{next(`a`)} %d {next(`c`)}\n", next("b"))
    fmt.println(order)
}
"#,
    );
    assert_eq!(out, "1 3 2\n[a c b]\n");
}
