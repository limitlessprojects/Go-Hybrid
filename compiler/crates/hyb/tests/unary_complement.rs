use std::{fs, process::Command};

fn check_build_run(tag: &str, text: &str, expected: &str) {
    let dir =
        std::env::temp_dir().join(format!("hyb-unary-complement-{tag}-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let source = dir.join("complement.hy");
    fs::write(&source, text).unwrap();
    let check = Command::new(env!("CARGO_BIN_EXE_hyb"))
        .arg("check")
        .arg(&source)
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
    let binary = dir.join(format!("complement{}", std::env::consts::EXE_SUFFIX));
    let build = Command::new(env!("CARGO_BIN_EXE_hyb"))
        .arg("build")
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(&binary).output().unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8(run.stdout).unwrap(), expected);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn documented_unary_complement_builds_and_runs_with_go_integer_semantics() {
    check_build_run(
        "integers",
        r#"
type octet = u8
const top octet = 254
fn main() {
    x := 3
    fmt.println(^x, ^^x, ^x ^ 2, ^(x ^ 2))
    fmt.println(^0, ^u8(0), ^u16(0), ^i8(0))
    var value u8 = 1
    fmt.println((^value) + 2)
    fmt.println(u8(^0), u8(^u16(0)))
    var array [^top]int
    fmt.println(len(array))
    const local u8 = 254
    var second [^local]int
    fmt.println(len(second))
}
"#,
        "-4 3 -2 -2\n-1 255 65535 -1\n0\n255 255\n1\n1\n",
    );
}

#[test]
fn complement_conversion_folds_do_not_erase_shadowed_function_calls() {
    check_build_run(
        "shadow",
        r#"
fn u8(x int) int { return x + 1 }
fn main() {
    u16 := fn(x int) int { return x + 2 }
    fmt.println(i8(^u8(0)), i8(^u16(0)))
    fmt.println(u8(0), u16(0))
}
"#,
        "-2 -3\n1 2\n",
    );
}
