use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Case(PathBuf);

impl Case {
    fn new(source: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "hyb-method-expr-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).expect("create unique case directory");
        fs::write(dir.join("case.hy"), source).unwrap();
        Self(dir)
    }

    fn check(&self) -> String {
        let output = Command::new(env!("CARGO_BIN_EXE_hyb"))
            .arg("check")
            .arg("--json")
            .arg("case.hy")
            .current_dir(&self.0)
            .output()
            .expect("run hyb check");
        String::from_utf8(output.stdout).unwrap()
    }
}

impl Drop for Case {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn rejects(source: &str, code: &str) {
    let got = Case::new(source).check();
    assert!(got.contains(code), "expected {code}, got: {got}");
}

#[test]
fn method_expr_interproc_alias_freeze() {
    rejects(
        r#"struct ops {}

fn (o ops) link(b map[string]map[string]int, m map[string]int) {
    b["a"] = m
}

fn worker(b map[string]map[string]int, done chan int) {
    for i in 0..20000 {
        b["a"]["k"] = i
    }
    done <- 1
}

fn main() {
    b := map[string]map[string]int{}
    m := map[string]int{"k": 0}
    ops.link(ops{}, b, m)
    done := make(chan int, 1)
    task worker(move b, done)
    for i in 0..20000 {
        m["k"] = i
    }
    _ = <-done
}
"#,
        "HYB-E3003",
    );
}

#[test]
fn method_expr_direct_const_receiver() {
    rejects(
        r#"struct cell { m map[string]int }
fn (c cell) set_k(v int) { c.m["k"] = v }
const g = cell{m: map[string]int{"k": 1}}

fn reader(done chan int) {
    sum := 0
    for i in 0..20000 { sum += g.m["k"] + i * 0 }
    _ = sum
    done <- 1
}

fn main() {
    done := make(chan int, 1)
    task reader(done)
    for i in 0..20000 { cell.set_k(g, i) }
    _ = <-done
}
"#,
        "HYB-E3006",
    );
}

#[test]
fn method_expr_bound_const_receiver() {
    rejects(
        r#"struct cell { m map[string]int }
fn (c cell) set_k(v int) { c.m["k"] = v }
const g = cell{m: map[string]int{"k": 1}}

fn reader(done chan int) {
    sum := 0
    for i in 0..20000 { sum += g.m["k"] + i * 0 }
    _ = sum
    done <- 1
}

fn main() {
    done := make(chan int, 1)
    task reader(done)
    f := cell.set_k
    for i in 0..20000 { f(g, i) }
    _ = <-done
}
"#,
        "HYB-E3006",
    );
}

#[test]
fn method_expr_non_owned_param_smuggle() {
    rejects(
        r#"struct ops {}

fn (o ops) link(b map[string]map[string]int, m map[string]int) {
    b["a"] = m
}

fn worker(b map[string]map[string]int, done chan int) {
    for i in 0..20000 {
        b["a"]["k"] = i
    }
    done <- 1
}

fn smuggle(m map[string]int, done chan int) {
    b := map[string]map[string]int{}
    ops.link(ops{}, b, m)
    task worker(move b, done)
}

fn main() {
    m := map[string]int{"k": 0}
    done := make(chan int, 1)
    smuggle(m, done)
    for i in 0..20000 { m["k"] = i }
    _ = <-done
}
"#,
        "HYB-E3004",
    );
}

#[test]
fn method_expr_without_boundary_stays_accepted() {
    let got = Case::new(
        r#"struct cell { m map[string]int }
fn (c cell) set_k(v int) { c.m["k"] = v }

fn main() {
    c := cell{m: map[string]int{"k": 0}}
    cell.set_k(c, 3)
    f := cell.set_k
    f(c, 4)
    v := c.m["k"]
    _ = v
}
"#,
    )
    .check();
    assert!(
        got.contains("\"verdict\": \"accept\""),
        "benign method expression must stay accepted, got: {got}"
    );
}
