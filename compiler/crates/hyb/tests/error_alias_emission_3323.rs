//! Follow-up to #3323/#3325: T25 error aliases must preserve E4 emission
//! and the value/error split for both function and closure calls.
//! Use the Go compiler/runtime already required by make test-quick, so a wrong
//! assignment arity cannot pass merely because emission returned Ok.
use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "hybrid-error-alias-3323-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("create owned test directory");
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn builtin_error_aliases_build_and_propagate_through_functions_and_closures() {
    let cases: &[(&str, &str, &str)] = &[
        (
            "plain_control",
            r#"fn val() (int, error) {
    return 7, errors.new("sentinel")
}
fn outer() (int, error) {
    x := val()?
    return x, nil
}
fn main() {
    x, err := outer()
    fmt.println(x, err)
}
"#,
            "0 sentinel\n",
        ),
        (
            "caller_error_alias",
            r#"type failure = error
fn val() (int, error) {
    return 7, errors.new("sentinel")
}
fn outer() (int, failure) {
    x := val()?
    return x, nil
}
fn main() {
    x, err := outer()
    fmt.println(x, err)
}
"#,
            "0 sentinel\n",
        ),
        (
            "top_level_alias_callee",
            r#"type failure = error
fn step() failure {
    return errors.new("callee")
}
fn run() error {
    step()?
    return nil
}
fn main() {
    fmt.println(run())
}
"#,
            "callee\n",
        ),
        (
            "closure_alias_body",
            r#"type failure = error
type deep_failure = failure
fn val() (int, error) {
    return 7, errors.new("sentinel")
}
fn main() {
    f := fn() (int, deep_failure) {
        x := val()?
        return x, nil
    }
    x, err := f()
    fmt.println(x, err)
}
"#,
            "0 sentinel\n",
        ),
        (
            "closure_alias_callee",
            r#"type failure = error
fn main() {
    step := fn() failure {
        return nil
    }
    step()?
    fmt.println("ok")
}
"#,
            "ok\n",
        ),
    ];
    for &(name, source, expected) in cases {
        let ast = parse_file(name, source).expect("valid fixture must parse");
        let checked = check_file(&ast);
        assert_eq!(
            checked.verdict,
            Verdict::Accept,
            "{name}: {:?}",
            checked.diagnostics
        );
        let go = hy_emit::emit_go(&ast).unwrap_or_else(|e| panic!("{name}: {}", e.message));
        let dir = Temp::new();
        let input = dir.0.join("main.go");
        let program = dir
            .0
            .join(format!("program{}", std::env::consts::EXE_SUFFIX));
        fs::write(&input, go).expect("write emitted source");
        let built = Command::new("go")
            .arg("build")
            .arg("-o")
            .arg(&program)
            .arg(&input)
            .output()
            .expect("Go is required by the repository test environment");
        assert!(
            built.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&built.stderr)
        );
        let ran = Command::new(&program)
            .output()
            .expect("run emitted program");
        assert!(
            ran.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&ran.stderr)
        );
        assert_eq!(
            String::from_utf8(ran.stdout).expect("UTF-8 output"),
            expected,
            "{name}"
        );
    }
}

#[test]
fn non_error_alias_still_rejects_try_context() {
    let source = r#"type failure = int
fn val() (int, error) {
    return 7, errors.new("sentinel")
}
fn outer() (int, failure) {
    x := val()?
    return x, 0
}
fn main() {
    _, _ = outer()
}
"#;
    let ast = parse_file("non-error.hy", source).expect("control must parse");
    let checked = check_file(&ast);
    assert_eq!(checked.verdict, Verdict::Reject);
    assert_eq!(
        checked
            .diagnostics
            .iter()
            .filter(|d| d.code == "HYB-E2011")
            .count(),
        1
    );
}
