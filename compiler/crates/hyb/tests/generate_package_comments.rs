//! Go permits a line comment between `package` and its identifier.
//! Keep package inference consistent with the real Go toolchain.
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

struct Module(PathBuf);
impl Module {
    fn new(tag: &str, header: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "hyb-package-comments-{}-{}",
            tag,
            std::process::id()
        ));
        fs::create_dir(&root).expect("create fresh module");
        fs::write(
            root.join("go.mod"),
            "module example.com/comment-demo\n\ngo 1.26\n",
        )
        .unwrap();
        fs::write(root.join("app.go"), header).unwrap();
        fs::write(
            root.join("calc.hy"),
            "pub fn add(a int, b int) int { return a + b }\n",
        )
        .unwrap();
        Self(root)
    }
    fn hyb(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_hyb"))
            .args(args)
            .current_dir(&self.0)
            .output()
            .expect("run hyb")
    }
    fn go_test(&self) -> Output {
        Command::new("go")
            .args(["test", "./..."])
            .env("GOTOOLCHAIN", "local")
            .env("GOPROXY", "off")
            .env("GOSUMDB", "off")
            .env("GOFLAGS", "-p=2")
            .current_dir(&self.0)
            .output()
            .expect("run native Go control")
    }
}
impl Drop for Module {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn success(output: Output, step: &str) {
    assert!(
        output.status.success(),
        "{}: stdout={} stderr={}",
        step,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
fn round_trip(tag: &str, header: &str) {
    let m = Module::new(tag, header);
    // Establish independently that this is a valid Go package before generation.
    success(m.go_test(), "native header before generation");
    success(m.hyb(&["generate", "calc.hy"]), "generate");
    let output = m.0.join("calc.hy.go");
    let before = fs::read(&output).unwrap();
    assert!(String::from_utf8_lossy(&before).contains("package demo\n"));
    let modified = fs::metadata(&output).unwrap().modified().unwrap();
    success(m.hyb(&["generate", "--check", "calc.hy"]), "fresh check");
    assert_eq!(fs::read(&output).unwrap(), before);
    assert_eq!(fs::metadata(&output).unwrap().modified().unwrap(), modified);
    fs::write(m.0.join("calc_test.go"),
        "package demo\nimport \"testing\"\nfunc TestGeneratedAdd(t *testing.T) { if Add(2, 3) != 5 { t.Fatal(\"wrong addition\") } }\n").unwrap();
    success(m.go_test(), "generated addition executes in native Go");
}
#[test]
fn adjacent_line_comment_separates_package_keyword_and_name() {
    round_trip("lf", "package// package documentation\ndemo\n");
}
#[test]
fn crlf_line_comment_separates_package_keyword_and_name() {
    round_trip("crlf", "package// package documentation\r\ndemo\r\n");
}
#[test]
fn empty_adjacent_line_comment_is_a_separator() {
    round_trip("empty", "package//\ndemo\n");
}
#[test]
fn chained_line_and_block_comments_are_separators() {
    round_trip("chain", "package// first\n// second\n/* third */demo\n");
}
#[test]
fn leading_bom_and_license_do_not_hide_the_line_comment_separator() {
    round_trip("bom", "\u{feff}/* license */\npackage// 한글 주석\ndemo\n");
}
#[test]
fn existing_whitespace_and_block_comment_forms_remain_valid() {
    for (tag, header) in [
        ("space", "package demo\n"),
        ("space-line", "package // comment\ndemo\n"),
        ("block", "package/* comment */demo\n"),
    ] {
        round_trip(tag, header);
    }
}
#[test]
fn comment_separated_sibling_participates_in_package_agreement() {
    let m = Module::new("conflict", "package demo\n");
    fs::write(m.0.join("other.go"), "package// other package\nother\n").unwrap();
    assert!(
        !m.go_test().status.success(),
        "native Go must reject the mismatch"
    );
    for args in [
        vec!["generate", "calc.hy"],
        vec!["generate", "--check", "calc.hy"],
    ] {
        let result = m.hyb(&args);
        assert!(
            !result.status.success(),
            "conflicting packages must not generate"
        );
        assert!(String::from_utf8_lossy(&result.stderr).contains("disagree on the package"));
        assert!(!m.0.join("calc.hy.go").exists());
    }
}
#[test]
fn comment_without_a_package_name_remains_invalid() {
    let m = Module::new("missing", "package// no identifier");
    assert!(!m.go_test().status.success());
    let output = m.hyb(&["generate", "calc.hy"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("HYB-E9002"));
    assert!(!m.0.join("calc.hy.go").exists());
}
#[test]
fn keyword_prefix_is_not_a_package_clause() {
    let m = Module::new("keyword-prefix", "packageName// not the keyword\ndemo\n");
    assert!(!m.go_test().status.success());
    let output = m.hyb(&["generate", "calc.hy"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("HYB-E9002"));
    assert!(!m.0.join("calc.hy.go").exists());
}
