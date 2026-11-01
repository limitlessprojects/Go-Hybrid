//! Package inference must follow Go's separation of production and test files.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

struct Module(PathBuf);

impl Module {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "hyb-generate-packages-{}-{}",
            tag,
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let module = Self(root);
        module.write("go.mod", "module example.com/demo\n\ngo 1.26\n");
        module.write("calc.hy", "pub fn add(a int, b int) int { return a + b }\n");
        module
    }

    fn write(&self, name: &str, text: &str) {
        fs::write(self.0.join(name), text).unwrap();
    }

    fn hyb(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_hyb"))
            .args(args)
            .current_dir(&self.0)
            .output()
            .expect("run hyb")
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
fn generate_and_check_with_internal_and_external_tests() {
    let m = Module::new("external");
    m.write("app.go", "package demo\n");
    m.write("internal_test.go", "package demo\n");
    // Sort the external package first, so inference cannot accidentally use it.
    m.write(
        "aaa_test.go",
        "package demo_test\n\nimport (\n\"testing\"\n\"example.com/demo\"\n)\n\n\
         func TestGeneratedAddition(t *testing.T) {\n\
         if got := demo.Add(2, 3); got != 5 { t.Fatalf(\"got %d, want 5\", got) }\n}\n",
    );

    assert_success(&m.hyb(&["generate", "calc.hy"]));
    let generated = fs::read_to_string(m.0.join("calc.hy.go")).unwrap();
    assert!(generated.lines().any(|line| line == "package demo"));
    assert_success(&m.hyb(&["generate", "--check", "."]));

    let out = Command::new("go")
        .args(["test", "./..."])
        .current_dir(&m.0)
        .output()
        .expect("run go test");
    assert_success(&out);
}

#[test]
fn test_only_siblings_do_not_select_a_production_package() {
    for package in ["demo", "demo_test"] {
        let m = Module::new(package);
        m.write("app_test.go", &format!("package {}\n", package));
        for args in [vec!["generate", "."], vec!["generate", "--check", "."]] {
            let out = m.hyb(&args);
            assert!(!out.status.success(), "test-only package must not emit");
            let stderr = String::from_utf8_lossy(&out.stderr);
            assert!(stderr.contains("HYB-E9002"), "{}", stderr);
            assert!(stderr.contains("non-test .go"), "{}", stderr);
            assert!(!m.0.join("calc.hy.go").exists());
        }
    }
}

#[test]
fn conflicting_non_test_packages_still_reject() {
    let m = Module::new("conflict");
    m.write("app.go", "package demo\n");
    m.write("other.go", "package other\n");
    m.write("app_test.go", "package demo_test\n");
    let out = m.hyb(&["generate", "."]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("disagree on the package"), "{}", stderr);
    assert!(!m.0.join("calc.hy.go").exists());
}

// The Go command ignores source basenames beginning with '.' or '_'. They
// can contain a different package (e.g. a saved example) without changing
// the production package selected by `go list` or `go test`.
fn assert_ignored_sibling(tag: &str, filename: &str) {
    let m = Module::new(tag);
    m.write("app.go", "package demo\n");
    m.write(filename, "package discarded\n");

    let listed = Command::new("go")
        .args(["list", "-f", "{{.Name}}", "."])
        .current_dir(&m.0)
        .output()
        .expect("run Go's independent package-selection oracle");
    assert_success(&listed);
    assert_eq!(String::from_utf8_lossy(&listed.stdout).trim(), "demo");

    assert_success(&m.hyb(&["generate", "calc.hy"]));
    let generated = fs::read_to_string(m.0.join("calc.hy.go")).unwrap();
    assert!(generated.lines().any(|line| line == "package demo"));
    assert_success(&m.hyb(&["generate", "--check", "."]));
    assert_eq!(
        fs::read_to_string(m.0.join("calc.hy.go")).unwrap(),
        generated
    );

    let out = Command::new("go")
        .args(["test", "./..."])
        .current_dir(&m.0)
        .output()
        .expect("compile the generated package with the ignored sibling present");
    assert_success(&out);
}

#[test]
fn underscore_prefixed_go_files_do_not_affect_package_inference() {
    assert_ignored_sibling("underscore-sibling", "_scratch.go");
}

#[test]
fn dot_prefixed_go_files_do_not_affect_package_inference() {
    assert_ignored_sibling("dot-sibling", ".scratch.go");
}

fn assert_ignored_only(tag: &str, filename: &str) {
    let m = Module::new(tag);
    m.write(filename, "package discarded\n");
    for args in [vec!["generate", "."], vec!["generate", "--check", "."]] {
        let out = m.hyb(&args);
        assert!(
            !out.status.success(),
            "ignored files cannot select a production package"
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("HYB-E9002"), "{}", stderr);
        assert!(!m.0.join("calc.hy.go").exists());
    }
}

#[test]
fn underscore_only_siblings_do_not_select_a_production_package() {
    assert_ignored_only("underscore-only", "_scratch.go");
}

#[test]
fn dot_only_siblings_do_not_select_a_production_package() {
    assert_ignored_only("dot-only", ".scratch.go");
}

#[test]
fn nonleading_dots_and_underscores_remain_package_authorities() {
    for (tag, filename) in [
        ("interior-underscore", "app_extra.go"),
        ("interior-dot", "app.extra.go"),
    ] {
        let m = Module::new(tag);
        m.write(filename, "package demo\n");
        assert_success(&m.hyb(&["generate", "."]));
        assert_success(&m.hyb(&["generate", "--check", "."]));
        m.write("conflict.go", "package other\n");
        let out = m.hyb(&["generate", "--check", "."]);
        assert!(!out.status.success());
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("disagree on the package"), "{}", stderr);
    }
}
