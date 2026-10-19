//! LX6 rune literals must retain their code points in buildable Go output.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn check_build_and_run(tag: &str, body: &str, expected: &str) {
    let dir =
        Scratch(std::env::temp_dir().join(format!("hyb-rune-{}-{}", tag, std::process::id())));
    let _ = fs::remove_dir_all(&dir.0);
    fs::create_dir_all(&dir.0).unwrap();
    let src = dir.0.join("runes.hy");
    fs::write(&src, format!("fn main() {{\n{body}\n}}\n")).unwrap();

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

    let bin = dir.0.join("runes");
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

    let run = Command::new(&bin).output().expect("run rune program");
    assert!(
        run.status.success(),
        "rune program failed: {:?}\nstderr: {}",
        run.status,
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8(run.stdout).unwrap(), expected);
}

#[test]
fn control_and_nonprintable_runes_build_with_correct_code_points() {
    check_build_and_run(
        "nonprintable",
        r#"
    fmt.println('\u{0}')
    fmt.println('\u{7}')
    fmt.println('\u{1b}')
    fmt.println('\u{10ffff}')
"#,
        "0\n7\n27\n1114111\n",
    );
}

#[test]
fn ordinary_and_escaped_runes_keep_their_code_points() {
    check_build_and_run(
        "ordinary",
        r#"
    fmt.println('a')
    fmt.println('\n')
    fmt.println('\'')
    fmt.println('\\')
    fmt.println('\u{1f600}')
"#,
        "97\n10\n39\n92\n128512\n",
    );
}
