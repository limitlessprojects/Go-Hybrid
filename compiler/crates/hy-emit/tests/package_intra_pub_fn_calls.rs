use hy_emit::{emit_go, emit_go_package};
use hy_syntax::parse_file;

#[test]
fn test_package_intra_pub_fn_calls_and_build() {
    let src = "package demo\nstruct box { val int }\npub fn bump_box(b *box) { b.val = b.val + 1 }\npub fn step_val(x int) (int, error) { return x + 1, nil }\npub fn id_val[t any](x t) t { return x }\npub fn ping_fn(n int) int { if n <= 0 { return 0 } return pong_fn(n - 1) }\npub fn pong_fn(n int) int { return ping_fn(n) }\npub fn helper_val(x int) int { return x * 2 }\npub fn entry_point() (int, error) {\n    defer helper_val(1)\n    task helper_val(2)\n    var b box\n    bump_box(b)\n    _ = id_val[u8](3)\n    f := helper_val\n    _ = f(4)\n    v := step_val(5)?\n    _ = ping_fn(1)\n    return helper_val(v), nil\n}\n";
    let ast = parse_file("demo.hy", src).expect("parses");
    let go = emit_go_package(&ast, "demo", "example.com/demo").expect("emits");

    for s in [
        "func BumpBox(b *box)",
        "func StepVal(x int)",
        "func IDVal[t any](x t) t",
        "func PingFn(n int) int",
        "func PongFn(n int) int",
        "func HelperVal(x int) int",
        "func EntryPoint()",
        "defer HelperVal(1)",
        "go HelperVal(2)",
        "BumpBox(&b)",
        "IDVal[uint8](3)",
        "f := HelperVal",
        "StepVal(5)",
        "PongFn(n - 1)",
        "PingFn(n)",
        "return HelperVal(v), nil",
    ] {
        assert!(go.contains(s), "missing `{s}` in:\n{go}");
    }

    let dir = std::env::temp_dir().join(format!("hy_pkg_test_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    std::fs::write(dir.join("go.mod"), "module example.com/demo\n\ngo 1.22\n").expect("mod");
    std::fs::write(dir.join("demo.go"), go.as_bytes()).expect("write");
    if let Ok(out) = std::process::Command::new("go").args(["build", "."]).current_dir(&dir).output() {
        assert!(
            out.status.success(),
            "go build failed on emitted package:\n---\n{}\n---\nstderr:\n{}",
            go,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_pub_fn_shadowing_and_program_mode() {
    let src = "package demo\npub fn helper_val(x int) int { return x }\npub fn caller(helper_val int) int { return helper_val }\npub fn local_caller() int {\n    helper_val := 7\n    return helper_val\n}\n";
    let ast = parse_file("demo.hy", src).expect("parses");
    let pkg_go = emit_go_package(&ast, "demo", "example.com/demo").expect("emits package");
    assert!(pkg_go.contains("func HelperVal(x int) int") && pkg_go.contains("func Caller(helper_val int) int"));
    assert!(pkg_go.contains("return helper_val") && pkg_go.contains("helper_val := 7"));

    let prog_go = emit_go(&ast).expect("emits program");
    assert!(prog_go.contains("func helper_val(x int) int") && prog_go.contains("func caller(helper_val int) int"));
    assert!(prog_go.contains("return helper_val"));
}
