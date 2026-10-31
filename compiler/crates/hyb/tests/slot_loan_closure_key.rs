//! Issue #2386 end to end: a closure that reassigns the key of a live O8
//! slot loan must not redirect the store-back. `s := &m["a"]`-by-key-`k`
//! writes back to `m["a"]` even after the closure moved `k` to "b"; before
//! the fix the accepted program printed `1 50` (m["b"] clobbered).

use std::fs;
use std::path::PathBuf;
use std::process::Command;

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const PROGRAM: &str = r#"struct svc {
    count int
}

fn before_loan() {
    k := "a"
    m := map[string]svc{"a": svc{count: 1}, "b": svc{count: 2}}
    setk := fn() {
        k = "b"
    }
    s := &m[k]
    setk()
    s.count = 50
    fmt.println("{k} {m["a"].count} {m["b"].count}")
}

fn inside_loan() {
    k := "a"
    m := map[string]svc{"a": svc{count: 1}, "b": svc{count: 2}}
    s := &m[k]
    setk := fn() {
        k = "b"
    }
    setk()
    s.count = 60
    fmt.println("{k} {m["a"].count} {m["b"].count}")
}

fn main() {
    before_loan()
    inside_loan()
}
"#;

#[test]
fn closure_key_write_does_not_redirect_the_store_back() {
    let dir = Scratch(std::env::temp_dir().join(format!("hyb-slot-key-{}", std::process::id())));
    let _ = fs::remove_dir_all(&dir.0);
    fs::create_dir_all(&dir.0).unwrap();
    let src = dir.0.join("slot_key.hy");
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

    let bin = dir.0.join("slot_key");
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

    let run = Command::new(&bin).output().expect("run slot_key program");
    assert!(
        run.status.success(),
        "program failed: {:?}\nstderr: {}",
        run.status,
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8(run.stdout).unwrap(), "b 50 2\nb 60 2\n");
}
