//! Package mode lowers top-level `pub fn` declarations to exported PascalCase.
//! Intra-package references must use the same Go name (#4433), while program
//! mode and shadowed bindings keep snake_case.

use hy_emit::{emit_go, emit_go_package};
use hy_syntax::parse_file;

#[test]
fn package_mode_renames_pub_fn_references_in_every_position() {
    let src = "\
struct box { val int }
pub fn bump_box(b *box) { b.val = b.val + 1 }
pub fn step_val(x int) (int, error) { return x + 1, nil }
pub fn id_val[t any](x t) t { return x }
pub fn ping_fn(n int) int { if n <= 0 { return 0 } return pong_fn(n - 1) }
pub fn pong_fn(n int) int { return ping_fn(n) }
pub fn helper_val(x int) int { return x * 2 }
pub fn entry_point() (int, error) {
    defer helper_val(1)
    task helper_val(2)
    var b box
    bump_box(&b)
    _ = id_val[u8](3)
    f := helper_val
    _ = f(4)
    v := step_val(5)?
    _ = ping_fn(1)
    return helper_val(v), nil
}
";
    let go = emit_go_package(&parse_file("demo.hy", src).unwrap(), "demo", "example.com/demo")
        .expect("package emission");
    for want in [
        "func BumpBox(b *box)",
        "func StepVal(x int) (int, error)",
        "func IDVal[t any](x t) t",
        "func PingFn(n int) int",
        "func PongFn(n int) int",
        "func HelperVal(x int) int",
        "func EntryPoint() (int, error)",
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
        assert!(go.contains(want), "missing `{want}` in:\n{go}");
    }

    // The emitted package must be ordinary buildable Go when a toolchain is
    // available (CI always has one; local sandboxes may not).
    let dir = std::env::temp_dir().join(format!("hy-pkg-pubfn-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("go.mod"), "module example.com/demo\n\ngo 1.22\n").unwrap();
    std::fs::write(dir.join("demo.go"), go.as_bytes()).unwrap();
    if let Ok(out) = std::process::Command::new("go")
        .args(["build", "."])
        .current_dir(&dir)
        .output()
    {
        assert!(
            out.status.success(),
            "emitted package does not build:\n{}\n{}",
            go,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn shadowed_pub_fn_names_stay_lowercase() {
    let src = "\
pub fn helper_val(x int) int { return x }
pub fn caller(helper_val int) int { return helper_val }
pub fn local_caller() int {
    helper_val := 7
    return helper_val
}
";
    let ast = parse_file("demo.hy", src).unwrap();
    let pkg = emit_go_package(&ast, "demo", "example.com/demo").expect("package emission");
    assert!(pkg.contains("func HelperVal(x int) int"), "{pkg}");
    assert!(pkg.contains("func Caller(helper_val int) int"), "{pkg}");
    assert!(pkg.contains("return helper_val"), "{pkg}");
    assert!(pkg.contains("helper_val := 7"), "{pkg}");

    // Program mode has no exported pub-fn surface: declarations and uses
    // stay snake_case.
    let prog = emit_go(&ast).expect("program emission");
    assert!(prog.contains("func helper_val(x int) int"), "{prog}");
    assert!(prog.contains("func caller(helper_val int) int"), "{prog}");
    assert!(prog.contains("return helper_val"), "{prog}");
}
