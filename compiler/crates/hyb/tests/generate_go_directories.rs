//! Directory operands are directories even when their names end in `.go`.
//! Ordinary forwarded Go-source arguments remain ignored by `generate`.
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture(PathBuf);

impl Fixture {
    fn new(tag: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "hyb-generate-go-directory-{tag}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        let f = Self(root);
        for dir in ["ordinary", "feature.go"] {
            let p = f.0.join(dir);
            fs::create_dir(&p).unwrap();
            fs::write(p.join("go.mod"), "module example.com/demo\n\ngo 1.26\n").unwrap();
            fs::write(p.join("app.go"), "package demo\n").unwrap();
            fs::write(
                p.join("calc.hy"),
                "pub fn add(a int, b int) int { return a + b }\n",
            )
            .unwrap();
        }
        f
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_hyb"))
            .args(args)
            .current_dir(&self.0)
            .output()
            .expect("run hyb")
    }

    fn generated(&self, dir: &str) -> PathBuf {
        self.0.join(dir).join("calc.hy.go")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn success(out: &Output) {
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn explicit_go_suffixed_directory_generates_and_checks() {
    let f = Fixture::new("explicit");
    success(&f.run(&["generate", "feature.go"]));
    let text = fs::read_to_string(f.generated("feature.go")).unwrap();
    assert!(text.contains("package demo"));
    assert!(text.contains("func Add("));
    success(&f.run(&["generate", "--check", "feature.go"]));
}

#[test]
fn go_suffixed_directory_is_not_omitted_from_mixed_operands() {
    let f = Fixture::new("mixed");
    let out = f.run(&["generate", "ordinary/calc.hy", "feature.go"]);
    success(&out);
    assert!(f.generated("ordinary").exists());
    assert!(f.generated("feature.go").exists());
    assert!(String::from_utf8_lossy(&out.stdout).contains("2 total"));
    success(&f.run(&["generate", "--check", "ordinary/calc.hy", "feature.go"]));
}

#[test]
fn go_suffixed_directory_after_option_terminator_is_supported() {
    let f = Fixture::new("terminator");
    fs::rename(f.0.join("feature.go"), f.0.join("-feature.go")).unwrap();
    success(&f.run(&["generate", "--", "-feature.go"]));
    assert!(f.generated("-feature.go").exists());
    success(&f.run(&["generate", "--check", "--", "-feature.go"]));
}

#[test]
fn directory_trailing_slash_remains_supported() {
    let f = Fixture::new("slash");
    success(&f.run(&["generate", "feature.go/"]));
    success(&f.run(&["generate", "--check", "feature.go/"]));
}

#[test]
fn forwarded_go_source_arguments_remain_ignored() {
    let f = Fixture::new("forwarded");
    success(&f.run(&["generate", "ordinary/calc.hy", "ordinary/app.go"]));
    success(&f.run(&[
        "generate",
        "--check",
        "--",
        "ordinary/app.go",
        "ordinary/calc.hy",
    ]));
    assert!(!f.generated("feature.go").exists());
}

#[test]
fn only_forwarded_go_source_keeps_usage_error() {
    let f = Fixture::new("go-only");
    let out = f.run(&["generate", "ordinary/app.go"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("usage: hyb generate"));
    assert!(!f.generated("ordinary").exists());
}

#[test]
fn check_includes_missing_output_in_go_suffixed_directory() {
    let f = Fixture::new("check-mixed");
    success(&f.run(&["generate", "ordinary/calc.hy"]));
    let before = fs::read(f.generated("ordinary")).unwrap();
    let out = f.run(&["generate", "--check", "ordinary/calc.hy", "feature.go"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("feature.go/calc.hy.go"));
    assert!(!f.generated("feature.go").exists());
    assert_eq!(fs::read(f.generated("ordinary")).unwrap(), before);
}

#[test]
fn unknown_option_after_directory_is_rejected_before_generation() {
    let f = Fixture::new("invalid-option");
    let out = f.run(&["generate", "feature.go", "--not-an-option"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown generate option"));
    assert!(!f.generated("feature.go").exists());
}

#[test]
fn binding_prefix_value_is_not_treated_as_a_directory_operand() {
    let f = Fixture::new("prefix");
    success(&f.run(&["generate", "--binding-prefix", "feature.go", "ordinary"]));
    assert!(f.generated("ordinary").exists());
    assert!(!f.generated("feature.go").exists());
}

#[cfg(unix)]
#[test]
fn explicit_go_suffixed_directory_link_is_supported() {
    let f = Fixture::new("root-link");
    std::os::unix::fs::symlink("feature.go", f.0.join("alias.go")).unwrap();
    success(&f.run(&["generate", "alias.go"]));
    assert!(f.generated("feature.go").exists());
    success(&f.run(&["generate", "--check", "alias.go"]));
}
