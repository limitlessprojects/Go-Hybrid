//! Reuse tool discovery within one generate invocation, never its formatted output.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Module {
    root: PathBuf,
    gofmt: PathBuf,
}

impl Module {
    fn new(tag: &str, files: usize) -> Self {
        let root = std::env::temp_dir().join(format!(
            "hyb-generate-formatter-{}-{}",
            tag,
            std::process::id()
        ));
        let goroot = Command::new("go")
            .args(["env", "GOROOT"])
            .output()
            .expect("locate the real Go formatter");
        assert!(goroot.status.success(), "{goroot:?}");
        let gofmt = Path::new(String::from_utf8(goroot.stdout).unwrap().trim()).join("bin/gofmt");
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::create_dir_all(root.join("goroot/bin")).unwrap();
        fs::write(root.join("app.go"), "package demo\n").unwrap();
        for i in 0..files {
            fs::write(
                root.join(format!("value{i}.hy")),
                format!("fn value{i}() int {{ return {i} }}\n"),
            )
            .unwrap();
        }
        script(
            &root.join("bin/go"),
            "printf 'go %s\\n' \"$*\" >> \"$HYB_TEST_TRACE\"\n\
             if [ \"$1\" = list ]; then\n\
             if [ \"$HYB_TEST_LIST_FAIL\" != '' ]; then echo 'selection failed' >&2; exit 23; fi\n\
             exec \"$HYB_TEST_REAL_GO\" \"$@\"\n\
             fi\n\
             printf '%s\\n' \"$HYB_TEST_GOROOT\"\n",
        );
        script(
            &root.join("goroot/bin/gofmt"),
            "printf 'primary\\n' >> \"$HYB_TEST_TRACE\"\n\
             if [ \"$HYB_TEST_FAIL\" != '' ]; then /bin/cat >/dev/null; exit 1; fi\n\
             exec \"$HYB_TEST_REAL_GOFMT\" \"$@\"\n",
        );
        script(
            &root.join("bin/gofmt"),
            "printf 'fallback\\n' >> \"$HYB_TEST_TRACE\"\n\
             exec \"$HYB_TEST_REAL_GOFMT\" \"$@\"\n",
        );
        Self { root, gofmt }
    }

    fn generate(&self, args: &[&str], fail: &str) -> Output {
        fs::write(self.root.join("trace"), "").unwrap();
        let mut paths = vec![self.root.join("bin")];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
        Command::new(env!("CARGO_BIN_EXE_hyb"))
            .arg("generate")
            .args(args)
            .current_dir(&self.root)
            .env("PATH", std::env::join_paths(paths).unwrap())
            .env("HYB_TEST_TRACE", self.root.join("trace"))
            .env("HYB_TEST_GOROOT", self.root.join("goroot"))
            .env("HYB_TEST_REAL_GOFMT", &self.gofmt)
            .env("HYB_TEST_REAL_GO", self.gofmt.with_file_name("go"))
            .env(
                "HYB_TEST_LIST_FAIL",
                if fail == "selection" { "1" } else { "" },
            )
            .env("HYB_TEST_FAIL", fail)
            .output()
            .expect("run actual hyb generate")
    }

    fn trace(&self) -> String {
        fs::read_to_string(self.root.join("trace")).unwrap()
    }

    fn assert_canonical(&self, files: usize) {
        for i in 0..files {
            let path = self.root.join(format!("value{i}.hy.go"));
            let text = fs::read_to_string(&path).unwrap();
            assert!(text.contains(&format!("func value{i}() int")), "{text}");
            let formatted = Command::new(&self.gofmt).arg(&path).output().unwrap();
            assert!(formatted.status.success(), "{formatted:?}");
            assert_eq!(formatted.stdout, text.as_bytes());
        }
    }
}

impl Drop for Module {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn script(path: &Path, body: &str) {
    fs::write(path, format!("#!/bin/sh\n{body}")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn each_invocation_locates_once_but_formats_every_file() {
    let m = Module::new("many", 3);
    for args in [vec!["."], vec!["--check", "."]] {
        let out = m.generate(&args, "");
        assert!(out.status.success(), "{out:?}");
        m.assert_canonical(3);
        assert_eq!(
            m.trace(),
            "go list -e -find -json .\ngo env GOROOT\nprimary\nprimary\nprimary\n"
        );
    }
    fs::write(m.root.join("value1.hy"), "fn value1() int { return 99 }\n").unwrap();
    let out = m.generate(&["--check", "."], "");
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("value1.hy.go"));
    assert_eq!(
        m.trace(),
        "go list -e -find -json .\ngo env GOROOT\nprimary\nprimary\nprimary\n"
    );
    assert!(!fs::read_to_string(m.root.join("value1.hy.go"))
        .unwrap()
        .contains("99"));
}

#[test]
fn primary_failure_still_tries_path_formatter_for_every_file() {
    let m = Module::new("fallback", 2);
    let out = m.generate(&["."], "primary");
    assert!(out.status.success(), "{out:?}");
    m.assert_canonical(2);
    assert_eq!(
        m.trace(),
        "go list -e -find -json .\ngo env GOROOT\nprimary\nfallback\nprimary\nfallback\n"
    );
}

#[test]
fn invalid_input_does_not_start_go_or_a_formatter() {
    let m = Module::new("invalid", 1);
    fs::write(m.root.join("bad.hy"), "fn bad( {\n").unwrap();
    for args in [vec!["--chekc", "."], vec!["missing.hy"], vec!["bad.hy"]] {
        let out = m.generate(&args, "");
        assert!(!out.status.success(), "{out:?}");
        assert_eq!(m.trace(), "");
    }
}

#[test]
fn selection_failure_does_not_touch_existing_output_or_start_formatter() {
    let m = Module::new("selection-failure", 1);
    let generated = m.root.join("value0.hy.go");
    fs::write(
        &generated,
        "// Code generated by hyb from value0.hy. DO NOT EDIT.\n\npackage old\n",
    )
    .unwrap();
    let before = fs::read(&generated).unwrap();
    let modified = fs::metadata(&generated).unwrap().modified().unwrap();
    for args in [vec!["."], vec!["--check", "."]] {
        let out = m.generate(&args, "selection");
        assert!(!out.status.success(), "{out:?}");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains("selection failed"), "{err}");
        assert_eq!(m.trace(), "go list -e -find -json .\n");
        assert_eq!(fs::read(&generated).unwrap(), before);
        assert_eq!(
            fs::metadata(&generated).unwrap().modified().unwrap(),
            modified
        );
    }
}
