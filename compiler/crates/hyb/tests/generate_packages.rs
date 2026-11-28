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

#[test]
fn generate_ignores_sibling_directories_ending_in_dot_go() {
    let m = Module::new("sibling-dir");
    m.write("app.go", "package demo\n");
    // Create a sibling subdirectory explicitly named nested.go
    let nested_dir = m.0.join("nested.go");
    fs::create_dir_all(&nested_dir).unwrap();
    fs::write(nested_dir.join("other.go"), "package other\n").unwrap();

    let out = m.hyb(&["generate", "calc.hy"]);
    assert_success(&out);
    let gen = fs::read_to_string(m.0.join("calc.hy.go")).expect("read generated calc.hy.go");
    assert!(gen.contains("package demo"));
}

