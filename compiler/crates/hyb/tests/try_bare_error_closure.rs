//! `?` on a closure bound by name that returns bare `error` must build
//! and propagate (E1: "or bare `error`"; E3). Before the fix it passed
//! `hyb check` and failed `go build` with an assignment mismatch.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const PROGRAM: &str = r#"import "errors"

fn run(fail bool) error {
    step := fn() error {
        if fail {
            return errors.new("step failed")
        }
        return nil
    }
    step()?
    fmt.println("stepped")
    return nil
}

fn main() {
    fmt.println(run(false))
    fmt.println(run(true))
    check := fn() error {
        return nil
    }
    check()?
    fmt.println("checked")
}
"#;

#[test]
fn bare_error_closure_try_builds_and_propagates() {
    let dir = Scratch(std::env::temp_dir().join(format!("hyb-try-closure-{}", std::process::id())));
    let _ = fs::remove_dir_all(&dir.0);
    fs::create_dir_all(&dir.0).unwrap();
    let src = dir.0.join("try_closure.hy");
    fs::write(&src, PROGRAM).unwrap();

    let check = Command::new(env!("CARGO_BIN_EXE_hyb"))
        .arg("check")
        .arg(&src)
        .current_dir(&dir.0)
        .output()
        .expect("run hyb check");
    assert!(
        check.status.success(),
        "hyb check failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let bin = dir.0.join("try_closure");
    let build = Command::new(env!("CARGO_BIN_EXE_hyb"))
        .arg("build")
        .arg(&src)
        .arg("-o")
        .arg(&bin)
        .current_dir(&dir.0)
        .output()
        .expect("run hyb build");
    assert!(
        build.status.success(),
        "hyb build failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    let run = Command::new(&bin)
        .output()
        .expect("run try_closure program");
    assert!(
        run.status.success(),
        "program failed: {:?}\nstderr: {}",
        run.status,
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8(run.stdout).unwrap(),
        "stepped\n<nil>\nstep failed\nchecked\n"
    );
}
