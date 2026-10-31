//! Issue #2384 end to end: `defer f()?` passes `hyb check`, so it must
//! build (Row 13 BUILDABLE) and behave like Go's defer + E3 propagation:
//! receiver and arguments evaluated at the defer statement, the deferred
//! error stored into the error result at exit unless an earlier error is
//! already being returned, and main's E6 exit contract on a deferred error.

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

struct res {
    name string
}

fn (r *res) close() error {
    fmt.println("closing {r.name}")
    return errors.new("close {r.name} failed")
}

fn fail_if(n int) error {
    fmt.println("deferred saw {n}")
    if n > 0 {
        return errors.new("deferred failure")
    }
    return nil
}

fn deferred_error_wins_over_nil() (int, error) {
    n := 1
    defer fail_if(n)?
    n = 0
    return 7, nil
}

fn explicit_error_kept() error {
    defer (fail_if(1))?
    return errors.new("explicit failure")
}

fn no_error_stays_nil() error {
    defer (fail_if(0)?)
    return nil
}

fn receiver_now() error {
    r := &res{name: "first"}
    defer r.close()?
    r = &res{name: "second"}
    fmt.println("body uses {r.name}")
    return nil
}

fn main() {
    v, err := deferred_error_wins_over_nil()
    fmt.println("{v} {err}")
    fmt.println(explicit_error_kept())
    fmt.println(no_error_stays_nil())
    fmt.println(receiver_now())
    defer fail_if(2)?
    fmt.println("main body done")
}
"#;

#[test]
fn defer_try_builds_and_propagates_at_exit() {
    let dir = Scratch(std::env::temp_dir().join(format!("hyb-defer-try-{}", std::process::id())));
    let _ = fs::remove_dir_all(&dir.0);
    fs::create_dir_all(&dir.0).unwrap();
    let src = dir.0.join("defer_try.hy");
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

    let bin = dir.0.join("defer_try");
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

    let run = Command::new(&bin).output().expect("run defer_try program");
    let stdout = String::from_utf8(run.stdout).unwrap();
    let stderr = String::from_utf8(run.stderr).unwrap();
    assert_eq!(
        stdout,
        "deferred saw 1\n\
         7 deferred failure\n\
         deferred saw 1\n\
         explicit failure\n\
         deferred saw 0\n\
         <nil>\n\
         body uses second\n\
         closing first\n\
         close first failed\n\
         main body done\n\
         deferred saw 2\n"
    );
    // E6: main's deferred error exits 1 with `<prog>: <err>` on stderr.
    assert_eq!(run.status.code(), Some(1), "stderr: {stderr}");
    assert!(
        stderr.trim_end().ends_with(": deferred failure"),
        "{stderr}"
    );
}
