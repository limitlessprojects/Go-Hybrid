use std::fs;
use std::path::PathBuf;
use std::process::Command;

const CORPUS02: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tests/checker-corpus/02-shared-map-managed.hy"
);
const CORPUS15: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tests/checker-corpus/15-fanin-per-task-state.hy"
);

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("hyb-build-cli-{}-{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

fn hyb(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_hyb"))
        .args(args)
        .env("HYB_ALLOW_TOOLCHAIN_DRIFT", "1")
        .output()
        .expect("spawn hyb");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn build_deny_managed_blocks_managed_program_before_emitting_go() {
    let dir = scratch("deny-managed");
    let out_bin = dir.join("prog");
    let out_str = out_bin.to_str().unwrap();

    let (code, stdout, stderr) = hyb(&["build", CORPUS02, "--deny-managed", "--emit-go", "-o", out_str]);
    assert_eq!(
        code, 1,
        "expected exit 1 on managed program under `hyb build --deny-managed`, stdout={stdout}, stderr={stderr}"
    );
    assert!(
        stdout.contains("deny-managed") && stdout.contains("1 managed block(s)"),
        "expected deny-managed count diagnostic on stdout, got: {stdout}"
    );
    assert!(
        !dir.join("prog.go").exists(),
        "must not emit Go output when `--deny-managed` rejects managed blocks"
    );

    let clean_out = dir.join("clean");
    let clean_str = clean_out.to_str().unwrap();
    let (code_ok, stdout_ok, stderr_ok) =
        hyb(&["build", CORPUS15, "--deny-managed", "--emit-go", "-o", clean_str]);
    assert_eq!(
        code_ok, 0,
        "hatch-free program must succeed under `hyb build --deny-managed --emit-go`: stdout={stdout_ok}, stderr={stderr_ok}"
    );
    assert!(
        dir.join("clean.go").exists(),
        "expected Go source to be emitted for hatch-free program"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn build_cli_rejects_unknown_flags_missing_values_extra_files_and_unpaired_package_flags() {
    let (c_unknown, _, err_unknown) = hyb(&["build", CORPUS15, "--unknown-flag"]);
    assert_eq!(c_unknown, 2, "unknown flag must exit 2: {err_unknown}");
    assert!(err_unknown.contains("unknown option: --unknown-flag"), "{err_unknown}");

    let (c_missing_o, _, err_missing_o) = hyb(&["build", CORPUS15, "-o"]);
    assert_eq!(c_missing_o, 2, "dangling `-o` must exit 2: {err_missing_o}");
    assert!(err_missing_o.contains("option -o requires an argument"), "{err_missing_o}");

    let (c_two_files, _, err_two_files) = hyb(&["build", CORPUS15, CORPUS02]);
    assert_eq!(c_two_files, 2, "multiple input files must exit 2: {err_two_files}");
    assert!(
        err_two_files.contains("build expects exactly one input file"),
        "{err_two_files}"
    );

    let (c_pkg_only, _, err_pkg_only) = hyb(&["build", CORPUS15, "--package", "demo"]);
    assert_eq!(c_pkg_only, 2, "unpaired `--package` must exit 2: {err_pkg_only}");
    assert!(
        err_pkg_only.contains("--package and --out-dir must be used together"),
        "{err_pkg_only}"
    );

    let (c_dir_only, _, err_dir_only) = hyb(&["build", CORPUS15, "--out-dir", "/tmp/hyb-unused"]);
    assert_eq!(c_dir_only, 2, "unpaired `--out-dir` must exit 2: {err_dir_only}");
    assert!(
        err_dir_only.contains("--package and --out-dir must be used together"),
        "{err_dir_only}"
    );
}
