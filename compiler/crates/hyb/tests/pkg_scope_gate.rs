//! Issue #4222 — the sibling package-scope scan only runs where the checked
//! directory is a Go module unit (`go.mod` right there).
//!
//! A flat directory without a module is a pile of unrelated single-file
//! programs (the 135-program checker corpus under `tests/checker-corpus/`
//! is exactly that, every program declaring `fn main`), so the scan must
//! fail open there. Inside a module — the environment `go build ./...`
//! applies to — a same-named top-level declaration in a sibling `.hy`
//! or hand-written `.go` file is `X redeclared in this block`.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Inputs(PathBuf);

impl Inputs {
    /// An empty unique directory; fixtures write themselves per test.
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "hyb-pkg-scope-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).expect("create unique inputs directory");
        Self(root)
    }

    fn module(&self) -> &Self {
        fs::write(
            self.0.join("go.mod"),
            "module issue42\n\ngo 1.26\n",
        )
        .unwrap();
        self
    }

    fn check(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_hyb"))
            .arg("check")
            .args(args)
            .current_dir(&self.0)
            .output()
            .expect("run actual hyb CLI")
    }
}

impl Drop for Inputs {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("remove test-owned inputs");
    }
}

const A: &str = "package multi\n\nfn helper(n int) int {\n    return n + 1\n}\n";
const B: &str = "package multi\n\nfn helper(n int) int {\n    return n * 2\n}\n";
const MAX: &str = "fn main() {\n}\n";

#[test]
fn module_dir_rejects_sibling_hy_redeclaration() {
    let inputs = Inputs::new();
    let root = &inputs.0;
    fs::write(root.join("a.hy"), A).unwrap();
    fs::write(root.join("b.hy"), B).unwrap();
    inputs.module();
    let output = inputs.check(&["b.hy"]);
    assert!(
        !output.status.success(),
        "module dir: sibling redeclaration must fail: {:?}",
        output
    );
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        stdout.contains("HYB-E1003") && stdout.contains("a.hy"),
        "must name HYB-E1003 and the declaring sibling: {stdout}"
    );
}

#[test]
fn module_dir_rejects_sibling_go_redeclaration() {
    let inputs = Inputs::new();
    let root = &inputs.0;
    fs::write(root.join("target.hy"), "package main\n\nfn helper(n int) int {\n    return n << 1\n}\n").unwrap();
    fs::write(
        root.join("main.go"),
        "package main\n\nfunc helper(n int) int { return n }\n\nfunc main() {}\n",
    )
    .unwrap();
    inputs.module();
    let output = inputs.check(&["target.hy"]);
    assert!(
        !output.status.success(),
        "module dir: sibling .go redeclaration must fail: {:?}",
        output
    );
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        stdout.contains("HYB-E1003") && stdout.contains("main.go"),
        "must name HYB-E1003 and the .go sibling: {stdout}"
    );
}

#[test]
fn flat_dir_without_module_fails_open() {
    // Issue reproduction B minus the module: two .hy files in a bare
    // directory are independent programs (the corpus shape) — the check
    // must stay a plain single-file check.
    let inputs = Inputs::new();
    let root = &inputs.0;
    fs::write(root.join("a.hy"), A).unwrap();
    fs::write(root.join("b.hy"), B).unwrap();
    let output = inputs.check(&["b.hy"]);
    assert!(
        output.status.success(),
        "no module → no package block → must stay accepted: {:?}",
        output
    );
}

#[test]
fn corpus_shape_many_mains_without_module_stays_accepted() {
    // The deny_managed/class-args regression class: several single-file
    // programs (all declaring `fn main`) checked one at a time must not
    // reject each other.
    let inputs = Inputs::new();
    let root = &inputs.0;
    for name in ["one.hy", "two.hy", "three.hy"] {
        fs::write(root.join(name), MAX).unwrap();
    }
    for name in ["one.hy", "two.hy", "three.hy"] {
        let output = inputs.check(&[name]);
        assert!(
            output.status.success(),
            "{name} must check clean next to unrelated programs: {:?}",
            output
        );
    }
}

#[test]
fn module_dir_broken_sibling_never_blocks_the_check() {
    let inputs = Inputs::new();
    let root = &inputs.0;
    fs::write(root.join("b.hy"), B).unwrap();
    fs::write(root.join("broken.hy"), "package multi\n\nfn ( {\n").unwrap();
    inputs.module();
    let output = inputs.check(&["b.hy"]);
    assert!(
        output.status.success(),
        "a sibling that fails to parse is skipped (fail-open): {:?}",
        output
    );
}
