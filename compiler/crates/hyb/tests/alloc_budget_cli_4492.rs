//! CLI integration tests for Checked Allocation (`--report-alloc`, `--check-alloc`,
//! `--heap-budget`, and `hyb explain HYB-E6007` / `HYB-E6008`, issue #4492).

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Sandbox(PathBuf);

impl Sandbox {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "hyb-alloc-cli-4492-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).expect("create temp sandbox");
        Self(dir)
    }

    fn write(&self, name: &str, content: &str) {
        fs::write(self.0.join(name), content).expect("write test fixture");
    }

    fn hyb(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_hyb"))
            .args(args)
            .env("HYB_ALLOW_TOOLCHAIN_DRIFT", "1")
            .current_dir(&self.0)
            .output()
            .expect("spawn hyb")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn report_alloc_human_and_json_modes() {
    let sb = Sandbox::new();
    sb.write(
        "prog.hy",
        r#"
fn tally(xs []string) map[string]int {
    m := map[string]int{}
    for x in xs {
        m[x]++
    }
    return m
}

fn main() {
    got := tally(["a", "b"])
    _ = len(got)
}
"#,
    );

    let human = sb.hyb(&["check", "prog.hy", "--report-alloc"]);
    assert!(human.status.success(), "{human:?}");
    let stdout = String::from_utf8(human.stdout).unwrap();
    assert!(stdout.contains("alloc:"), "{stdout}");
    assert!(stdout.contains("fn tally: linear(1 * xs + 1)"), "{stdout}");
    assert!(stdout.contains("fn main: const("), "{stdout}");

    let json_out = sb.hyb(&["check", "prog.hy", "--report-alloc", "--json"]);
    assert!(json_out.status.success(), "{json_out:?}");
    let v: serde_json::Value = serde_json::from_slice(&json_out.stdout).expect("valid json");
    assert_eq!(v["summary"]["verdict"], "accept");
    assert_eq!(v["summary"]["alloc"]["total_sites"], 3);
    let fns = v["summary"]["alloc"]["functions"].as_array().unwrap();
    assert!(fns
        .iter()
        .any(|f| f["name"] == "tally" && f["checked_bound"] == "linear(1 * xs + 1)"));
}

#[test]
fn check_alloc_and_heap_budget_enforce_fail_closed() {
    let sb = Sandbox::new();
    sb.write(
        "bounded.hy",
        r#"
fn pure(x int) int {
    return x + 1
}

fn main() {
    _ = pure(4)
}
"#,
    );
    sb.write(
        "over_budget.hy",
        r#"
fn main() {
    xs := []int{1, 2, 3}
    ys := append(xs, 4)
    _ = len(ys)
}
"#,
    );
    sb.write(
        "unbounded.hy",
        r#"
fn drain(ch <-chan int) []int {
    var out []int
    for v in ch {
        out = append(out, v)
    }
    return out
}

fn main() {}
"#,
    );

    let pass0 = sb.hyb(&["check", "bounded.hy", "--heap-budget", "0"]);
    assert_eq!(pass0.status.code(), Some(0), "{pass0:?}");

    let fail_e6007 = sb.hyb(&["check", "over_budget.hy", "--heap-budget=1"]);
    assert_eq!(fail_e6007.status.code(), Some(1), "{fail_e6007:?}");
    let out_e6007 = String::from_utf8(fail_e6007.stdout).unwrap();
    assert!(out_e6007.contains("HYB-E6007"), "{out_e6007}");
    assert!(out_e6007.contains("help:"), "{out_e6007}");

    let pass_higher = sb.hyb(&["check", "over_budget.hy", "--heap-budget=2"]);
    assert_eq!(pass_higher.status.code(), Some(0), "{pass_higher:?}");

    let fail_e6008 = sb.hyb(&["check", "unbounded.hy", "--check-alloc"]);
    assert_eq!(fail_e6008.status.code(), Some(1), "{fail_e6008:?}");
    let out_e6008 = String::from_utf8(fail_e6008.stdout).unwrap();
    assert!(out_e6008.contains("HYB-E6008"), "{out_e6008}");
    assert!(out_e6008.contains("unbounded"), "{out_e6008}");

    // `hyb build --heap-budget` blocks emission before invoking Go.
    let build_fail = sb.hyb(&[
        "build",
        "over_budget.hy",
        "-o",
        "outbin",
        "--heap-budget",
        "0",
    ]);
    assert_eq!(build_fail.status.code(), Some(1), "{build_fail:?}");
    let build_stdout = String::from_utf8(build_fail.stdout).unwrap();
    assert!(build_stdout.contains("HYB-E6007"), "{build_stdout}");
    assert!(!sb.0.join("outbin").exists());
}

#[test]
fn managed_hatch_interacts_cleanly_with_heap_budget_and_deny_managed() {
    let sb = Sandbox::new();
    sb.write(
        "hatched.hy",
        r#"
fn main() {
    ch := make(chan int, 1)
    close(ch)
    managed("external backpressure bounds queue", "other") {
        var xs []int
        for v in ch {
            xs = append(xs, v)
        }
        _ = len(xs)
    }
}
"#,
    );

    // Checked region only allocates `make(chan int, 1)` -> Const(1).
    let ok = sb.hyb(&[
        "check",
        "hatched.hy",
        "--heap-budget",
        "1",
        "--report-alloc",
    ]);
    assert_eq!(ok.status.code(), Some(0), "{ok:?}");
    let stdout = String::from_utf8(ok.stdout).unwrap();
    assert!(stdout.contains("program bound: const(1)"), "{stdout}");

    // Adding `--deny-managed` rejects the hatch by policy.
    let denied = sb.hyb(&[
        "check",
        "hatched.hy",
        "--heap-budget",
        "1",
        "--deny-managed",
    ]);
    assert_eq!(denied.status.code(), Some(1), "{denied:?}");
}

#[test]
fn cli_budget_validation_errors_before_file_io() {
    let sb = Sandbox::new();
    for bad_args in [
        vec!["check", "missing.hy", "--heap-budget"],
        vec!["check", "--heap-budget", "--json", "missing.hy"],
        vec!["check", "missing.hy", "--heap-budget=not_a_budget"],
        vec!["check", "missing.hy", "--heap-budget==5"],
    ] {
        let out = sb.hyb(&bad_args);
        assert_eq!(out.status.code(), Some(2), "args={bad_args:?}, out={out:?}");
        assert!(out.stdout.is_empty());
        let stderr = String::from_utf8(out.stderr).unwrap();
        assert!(stderr.contains("usage: hyb check"), "{stderr}");
        assert!(!stderr.contains("error reading"), "{stderr}");
    }
}

#[test]
fn explain_pages_exist_for_e6007_and_e6008() {
    let sb = Sandbox::new();
    for (code, slug) in [
        ("HYB-E6007", "heap-budget-exceeded"),
        ("HYB-E6008", "heap-budget-unbounded"),
    ] {
        let out = sb.hyb(&["explain", code]);
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        let page = String::from_utf8(out.stdout).unwrap();
        assert!(page.contains(code), "{page}");
        assert!(page.contains("fixes:"), "{page}");

        let by_slug = sb.hyb(&["explain", slug]);
        assert_eq!(by_slug.status.code(), Some(0), "{by_slug:?}");
    }
}
