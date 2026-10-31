//! Package authorities must exclude siblings Go ignores for this build.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

struct Module(PathBuf);

impl Module {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "hyb-build-constraints-{}-{}",
            tag,
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let m = Self(root);
        m.write("go.mod", "module example.com/constraint-demo\n\ngo 1.26\n");
        m.write(
            "app.go",
            "package demo\n\nfunc Existing() int { return Add(2, 3) }\n",
        );
        m.write("calc.hy", "pub fn add(a int, b int) int { return a + b }\n");
        m.write(
            "app_test.go",
            "package demo\n\nimport \"testing\"\n\nfunc TestGenerated(t *testing.T) {\n\
             if got := Existing(); got != 5 { t.Fatalf(\"got %d, want 5\", got) }\n}\n",
        );
        m
    }

    fn write(&self, name: &str, text: &str) {
        fs::write(self.0.join(name), text).unwrap();
    }

    fn command(&self, program: &str, args: &[&str], flags: &str) -> Output {
        Command::new(program)
            .args(args)
            .current_dir(&self.0)
            .env("GO111MODULE", "auto")
            .env("GOWORK", "off")
            .env("GOFLAGS", flags)
            .env("GOPROXY", "off")
            .output()
            .expect("run fixture command")
    }

    fn hyb(&self, args: &[&str], flags: &str) -> Output {
        self.command(env!("CARGO_BIN_EXE_hyb"), args, flags)
    }

    fn go_selection(&self, flags: &str) -> serde_json::Value {
        let out = self.command("go", &["list", "-e", "-find", "-json", "."], flags);
        assert_success(&out);
        serde_json::from_slice(&out.stdout).expect("Go's package JSON")
    }

    fn assert_ignored(&self, name: &str) {
        let package = self.go_selection("-p=2");
        assert!(
            package["IgnoredGoFiles"]
                .as_array()
                .expect("ignored Go file list")
                .iter()
                .any(|f| f.as_str() == Some(name)),
            "Go did not exclude {}: {}",
            name,
            package
        );
    }

    fn assert_generation_and_go_test(&self) {
        let module_before = fs::read(self.0.join("go.mod")).ok();
        assert_success(&self.hyb(&["generate", "calc.hy"], "-p=2"));
        let path = self.0.join("calc.hy.go");
        let generated = fs::read(&path).unwrap();
        let modified = fs::metadata(&path).unwrap().modified().unwrap();
        assert!(String::from_utf8_lossy(&generated)
            .lines()
            .any(|line| line == "package demo"));
        assert_success(&self.hyb(&["generate", "--check", "calc.hy"], "-p=2"));
        assert_eq!(fs::read(&path).unwrap(), generated);
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
        assert_success(&self.command("go", &["test", "-count=1", "./..."], "-p=2"));
        assert_eq!(fs::read(self.0.join("go.mod")).ok(), module_before);
        assert!(!self.0.join("go.sum").exists());
    }
}

impl Drop for Module {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn assert_success(out: &Output) {
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn modern_ignore_generator_does_not_conflict_with_package() {
    let m = Module::new("modern");
    m.write("aaa_generator.go", "//go:build ignore\n\npackage main\n");
    m.assert_ignored("aaa_generator.go");
    m.assert_generation_and_go_test();
}

#[test]
fn legacy_constraint_generator_does_not_conflict_with_package() {
    let m = Module::new("legacy");
    m.write(
        "aaa_generator.go",
        "// +build hyb_constraint_disabled\n\npackage main\n",
    );
    m.assert_ignored("aaa_generator.go");
    m.assert_generation_and_go_test();
}

#[test]
fn opposite_platform_sibling_does_not_conflict_with_package() {
    let m = Module::new("platform");
    let out = m.command("go", &["env", "GOOS"], "-p=2");
    assert_success(&out);
    let other = if String::from_utf8_lossy(&out.stdout).trim() == "windows" {
        "linux"
    } else {
        "windows"
    };
    let name = format!("aaa_generator_{}.go", other);
    m.write(&name, "package main\n");
    m.assert_ignored(&name);
    m.assert_generation_and_go_test();
}

#[test]
fn excluded_only_siblings_cannot_choose_the_package_or_rewrite_output() {
    let m = Module::new("excluded-only");
    fs::remove_file(m.0.join("app.go")).unwrap();
    fs::remove_file(m.0.join("app_test.go")).unwrap();
    m.write("generator.go", "//go:build ignore\n\npackage main\n");
    m.assert_ignored("generator.go");
    for existing in [false, true] {
        let path = m.0.join("calc.hy.go");
        if existing {
            m.write(
                "calc.hy.go",
                "// Code generated by hyb from calc.hy. DO NOT EDIT.\n\npackage old\n",
            );
        }
        let before = fs::read(&path).ok();
        let modified = fs::metadata(&path).ok().and_then(|m| m.modified().ok());
        for args in [
            vec!["generate", "calc.hy"],
            vec!["generate", "--check", "calc.hy"],
        ] {
            let out = m.hyb(&args, "-p=2");
            assert!(!out.status.success(), "excluded-only siblings must reject");
            let err = String::from_utf8_lossy(&out.stderr);
            assert!(err.contains("HYB-E9002"), "{}", err);
            assert_eq!(fs::read(&path).ok(), before);
            assert_eq!(
                fs::metadata(&path).ok().and_then(|m| m.modified().ok()),
                modified
            );
        }
    }
}

#[test]
fn enabled_custom_tag_retains_genuine_package_conflicts() {
    let m = Module::new("enabled-tag");
    m.write(
        "aaa_generator.go",
        "//go:build hyb_constraint_enabled\n\npackage main\n",
    );
    let flags = "-p=2 -tags=hyb_constraint_enabled";
    let package = m.go_selection(flags);
    assert!(package["GoFiles"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f.as_str() == Some("aaa_generator.go")));
    let out = m.hyb(&["generate", "calc.hy"], flags);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("disagree on the package"), "{}", err);
    assert!(!m.0.join("calc.hy.go").exists());
}

#[test]
fn ignored_generator_still_works_without_a_go_module() {
    let m = Module::new("no-module");
    fs::remove_file(m.0.join("go.mod")).unwrap();
    m.write("aaa_generator.go", "//go:build ignore\n\npackage main\n");
    m.assert_ignored("aaa_generator.go");
    m.assert_generation_and_go_test();
}

#[test]
fn stale_generated_package_does_not_mask_build_constraint_exclusions() {
    let m = Module::new("stale-output");
    m.write("aaa_generator.go", "//go:build ignore\n\npackage main\n");
    m.write(
        "calc.hy.go",
        "// Code generated by hyb from calc.hy. DO NOT EDIT.\n\npackage old\n",
    );
    m.assert_ignored("aaa_generator.go");
    m.assert_generation_and_go_test();
}
