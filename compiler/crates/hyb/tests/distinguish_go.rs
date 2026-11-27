//! Falsification test for #4807's next milestone: take the probes emitted from
//! real `.hy` revisions, generate each revision through Hybrid's ordinary Go
//! emitter, build eight independent Go binaries, and require every accept/
//! reject answer to match the pack's answer key.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use serde_json::Value;

static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

struct TempRoot(PathBuf);

impl TempRoot {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "hyb-distinguish-go-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create differential test root");
        Self(path)
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../spikes/w4807-recognizer-probe-pack/hy")
}

fn hyb(args: &[String]) -> Output {
    hyb_in(None, args)
}

fn hyb_in(cwd: Option<&Path>, args: &[String]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_hyb"));
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    command.output().expect("run hyb")
}

fn build_runner(root: &Path, label: &str, source: &Path) -> PathBuf {
    let module = root.join(label);
    let package = module.join("recognizer");
    fs::create_dir_all(&package).expect("create recognizer package");
    fs::write(module.join("go.mod"), "module probeexample\n\ngo 1.26\n").expect("write go.mod");
    fs::write(package.join("package.go"), "package recognizer\n").expect("write package marker");
    fs::copy(source, package.join("candidate.hy")).expect("copy recognizer source");

    let generated = hyb_in(
        Some(&module),
        &["generate".into(), "recognizer/candidate.hy".into()],
    );
    assert!(
        generated.status.success(),
        "hyb generate failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&generated.stdout),
        String::from_utf8_lossy(&generated.stderr)
    );
    fs::write(
        module.join("main.go"),
        r#"package main

import (
    "fmt"
    "os"
    "probeexample/recognizer"
)

func main() {
    if len(os.Args) != 2 {
        panic("usage: runner <ascii-input>")
    }
    if recognizer.Recognize([]byte(os.Args[1])) {
        fmt.Print("1")
    } else {
        fmt.Print("0")
    }
}
"#,
    )
    .expect("write Go runner");
    let binary = module.join("runner");
    let build = Command::new("go")
        .args(["build", "-o"])
        .arg(&binary)
        .arg(".")
        .current_dir(&module)
        .output()
        .expect("run go build");
    assert!(
        build.status.success(),
        "go build failed for {label}: stdout={}\nstderr={}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    binary
}

#[test]
fn emitted_probes_match_separately_built_go_binaries() {
    // Hosted CI has the pinned Go toolchain. Local Rust-only packaging jobs do
    // not, so this falsification test skips rather than replacing Go with a
    // weaker in-process oracle.
    if Command::new("go").arg("version").output().is_err() {
        eprintln!("SKIP: go toolchain unavailable");
        return;
    }

    let sources: Vec<PathBuf> = (0..8)
        .map(|code| fixture_root().join(format!("grammar-{code:03b}.hy")))
        .collect();
    let mut args = vec!["distinguish".to_string()];
    for (code, source) in sources.iter().enumerate() {
        args.push(format!("grammar-{code:03b}={}", source.display()));
    }
    let output = hyb(&args);
    assert!(
        output.status.success(),
        "hyb distinguish failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let pack: Value = serde_json::from_slice(&output.stdout).expect("pack JSON");
    let probes = pack["probes"].as_array().expect("probe array");
    assert_eq!(probes.len(), 3);

    let started = Instant::now();
    let root = TempRoot::new();
    for (candidate, source) in sources.iter().enumerate() {
        let runner = build_runner(&root.0, &format!("grammar-{candidate:03b}"), source);
        for probe in probes {
            let encoded = probe["input_base64"].as_str().expect("base64 input");
            let input = match encoded {
                "KzA=" => "+0",
                "MA==" => "0",
                "Xw==" => "_",
                other => panic!("unexpected probe base64 {other}"),
            };
            let observed = Command::new(&runner)
                .arg(input)
                .output()
                .expect("run compiled recognizer");
            assert!(
                observed.status.success(),
                "compiled recognizer failed: {}",
                String::from_utf8_lossy(&observed.stderr)
            );
            let observed_accept = observed.stdout == b"1";
            let expected_accept = probe["answers"][candidate]
                .as_bool()
                .expect("Boolean answer");
            assert_eq!(
                observed_accept, expected_accept,
                "candidate {candidate}, probe {:?}",
                input
            );
        }
    }
    eprintln!(
        "eight separately built Go recognizers agreed with all emitted probes in {:.3}s",
        started.elapsed().as_secs_f64()
    );
}
