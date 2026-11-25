//! E4 end to end: inside `main`'s E6 wrapper, a closure's `?` must return
//! through the CLOSURE's own error result, and the closure's own `return`
//! must not grow main's hidden `, nil`. Before the fix both shapes passed
//! `hyb check` and failed `go build` ("not enough return values" /
//! "too many return values").

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

fn fail() (int, error) {
    return 0, errors.new("inner failed")
}

fn val() (int, error) {
    return 5, nil
}

fn main() {
    step := fn() (int, error) {
        x := fail()?
        return x, nil
    }
    one := fn() int {
        return 1
    }
    v, err := step()
    fmt.println("{v} {err}")
    y := val()?
    fmt.println(y + one())
}
"#;

#[test]
fn closure_try_returns_through_the_closure_inside_main() {
    let dir = Scratch(std::env::temp_dir().join(format!("hyb-closure-try-{}", std::process::id())));
    let _ = fs::remove_dir_all(&dir.0);
    fs::create_dir_all(&dir.0).unwrap();
    let src = dir.0.join("closure_try.hy");
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

    let bin = dir.0.join("closure_try");
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
        .expect("run closure_try program");
    assert!(
        run.status.success(),
        "program failed: {:?}\nstderr: {}",
        run.status,
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8(run.stdout).unwrap(),
        "0 inner failed\n6\n"
    );
}
