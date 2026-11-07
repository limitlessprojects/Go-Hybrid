//! Cargo runs in the runtime crate, but explicit target paths belong to the caller.
#![cfg(all(unix, any(target_arch = "x86_64", target_arch = "aarch64")))]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

struct Runtime {
    root: PathBuf,
}

impl Runtime {
    fn new(tag: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("hyb vendor runtime {} {}", tag, std::process::id()));
        fs::create_dir_all(root.join("caller directory")).unwrap();
        fs::create_dir_all(root.join("runtime/rust/hyrt/src")).unwrap();
        fs::create_dir_all(root.join("runtime/go/binding")).unwrap();
        fs::write(root.join("runtime/rust/hyrt/src/lib.rs"), "").unwrap();
        fs::write(
            root.join("runtime/go/binding/binding.go"),
            "package binding\n",
        )
        .unwrap();

        let cpu = if cfg!(target_arch = "x86_64") {
            0x01000007
        } else {
            0x0100000c
        };
        let object: Vec<u8> = [0xfeedfacf_u32, cpu, 0, 1, 0, 0, 0, 0]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect();
        fs::write(root.join("object"), object).unwrap();
        let cargo = root.join("fake cargo");
        fs::write(
            &cargo,
            "#!/bin/sh\nset -eu\n\
             printf '%s\\n' \"${CARGO_TARGET_DIR-<unset>}\" >> \"$HYB_TEST_TRACE\"\n\
             target=\"${CARGO_TARGET_DIR:-target}\"\n\
             mkdir -p \"$target/release\"\n\
             cp \"$HYB_TEST_OBJECT\" \"$target/release/libhyrt.a\"\n",
        )
        .unwrap();
        fs::set_permissions(cargo, fs::Permissions::from_mode(0o755)).unwrap();
        Self {
            root: root.canonicalize().unwrap(),
        }
    }

    fn caller(&self) -> PathBuf {
        self.root.join("caller directory")
    }

    fn crate_dir(&self) -> PathBuf {
        self.root.join("runtime/rust/hyrt")
    }

    fn vendor(&self, target: Option<&Path>) {
        let mut command = Command::new(env!("CARGO_BIN_EXE_hyb"));
        command
            .args(["vendor-runtime", "vendored"])
            .current_dir(self.caller())
            .env("HYB_RUNTIME_DIR", self.root.join("runtime"))
            .env("HYB_CARGO_BIN", self.root.join("fake cargo"))
            .env("HYB_TEST_OBJECT", self.root.join("object"))
            .env("HYB_TEST_TRACE", self.root.join("trace"))
            .env_remove("CARGO_TARGET_DIR");
        if let Some(target) = target {
            command.env("CARGO_TARGET_DIR", target);
        }
        let output = command.output().expect("run actual hyb vendor-runtime");
        assert!(
            output.status.success(),
            "status: {}\nstdout: {}\nstderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert_eq!(
            fs::read(self.caller().join("vendored/hybridrt/lib/libhyrt.a")).unwrap(),
            fs::read(self.root.join("object")).unwrap(),
        );
    }

    fn check(&self, target: Option<&Path>, expected_target: &Path) {
        self.vendor(target);
        assert!(expected_target.join("release/libhyrt.a").is_file());
        let trace = fs::read_to_string(self.root.join("trace")).unwrap();
        let expected_env = if target.is_some() {
            expected_target.to_str().unwrap()
        } else {
            "<unset>"
        };
        assert_eq!(trace, format!("{expected_env}\n"));
        self.vendor(target);
        assert_eq!(
            fs::read_to_string(self.root.join("trace")).unwrap(),
            trace,
            "a fresh staticlib should be reused without running Cargo again",
        );
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn relative_target_dir_is_resolved_from_caller() {
    let runtime = Runtime::new("relative");
    let target = Path::new("cache directory");
    runtime.check(Some(target), &runtime.caller().join(target));
    assert!(!runtime.crate_dir().join(target).exists());
}

#[test]
fn parent_relative_target_dir_is_resolved_from_caller() {
    let runtime = Runtime::new("parent relative");
    let target = Path::new("../shared target");
    runtime.check(Some(target), &runtime.caller().join(target));
    assert!(!runtime.crate_dir().join(target).exists());
}

#[test]
fn absolute_target_dir_is_preserved() {
    let runtime = Runtime::new("absolute");
    let target = runtime.root.join("absolute target");
    runtime.check(Some(&target), &target);
}

#[test]
fn unset_target_dir_keeps_the_crate_local_default() {
    let runtime = Runtime::new("unset");
    runtime.check(None, &runtime.crate_dir().join("target"));
    assert!(!runtime.caller().join("target").exists());
}
