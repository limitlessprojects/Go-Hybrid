use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit_src(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("parses");
    emit_go(&ast).expect("emits")
}

#[test]
fn default_ident_minimal_repro_issue_1167() {
    let go = emit_src(
        "\
fn main() {
    default := make(chan int, 1)
    default <- 7
    v := <-default
    fmt.println(\"{v}\")
}
",
    );
    assert!(
        go.contains("default_ := make(chan int, 1)"),
        "expected mangled local channel binding, got:\n{go}"
    );
    assert!(
        go.contains("default_ <- 7"),
        "expected mangled channel send, got:\n{go}"
    );
    assert!(
        go.contains("v := <-default_"),
        "expected mangled channel recv, got:\n{go}"
    );
    assert!(
        !go.contains("default :="),
        "unmangled default keyword must not leak into Go:\n{go}"
    );

    // End-to-end go build & run verification for issue #1167 repro
    if std::process::Command::new("go")
        .arg("version")
        .output()
        .is_ok_and(|o| o.status.success())
    {
        let tmp = std::env::temp_dir().join(format!("hyb_emit_default_1167_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).expect("create temp dir");
        std::fs::write(tmp.join("go.mod"), "module dchan1167\n\ngo 1.21\n").expect("write go.mod");
        std::fs::write(tmp.join("main.go"), &go).expect("write main.go");
        let out = std::process::Command::new("go")
            .arg("run")
            .arg(".")
            .current_dir(&tmp)
            .output()
            .expect("invoke go run");
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(
            out.status.success(),
            "go run failed on emitted Go:\n{}\nstderr:\n{}",
            go,
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(stdout.trim(), "7");
    }
}

#[test]
fn default_ident_locals_and_params_and_returns() {
    let go = emit_src(
        "\
fn calc(default int) (result int) {
    var default_tmp int = default + 1
    return default_tmp
}

fn ret_named() (default int) {
    default = 42
    return default
}

fn main() {
    const default = 10
    default_val := calc(default)
    fmt.println(\"{default_val}\")
}
",
    );
    assert!(
        go.contains("func calc(default_ int) (result int)"),
        "parameter default must be mangled, got:\n{go}"
    );
    assert!(
        go.contains("func ret_named() (default_ int)"),
        "named return default must be mangled, got:\n{go}"
    );
    assert!(
        go.contains("default_ = 42"),
        "named return default assignment must be mangled, got:\n{go}"
    );
    assert!(
        go.contains("const default_ = 10"),
        "const default must be mangled, got:\n{go}"
    );
}

#[test]
fn default_ident_struct_and_field_and_fn() {
    let go = emit_src(
        "\
struct default {
    default int
}

fn default() int {
    return 1
}

fn main() {
    s := default{default: 42}
    v := s.default
    f := default()
    fmt.println(\"{v} {f}\")
}
",
    );
    assert!(
        go.contains("type default_ struct {"),
        "struct name default must be mangled, got:\n{go}"
    );
    assert!(
        go.contains("default_ int"),
        "field name default must be mangled, got:\n{go}"
    );
    assert!(
        go.contains("func default_() int"),
        "fn name default must be mangled, got:\n{go}"
    );
    assert!(
        go.contains("s := default_{default_: 42}"),
        "struct literal default must be mangled, got:\n{go}"
    );
    assert!(
        go.contains("v := s.default_"),
        "field access s.default must be mangled, got:\n{go}"
    );
    assert!(
        go.contains("f := default_()"),
        "call default() must be mangled, got:\n{go}"
    );
}

#[test]
fn default_ident_with_select_default_arm_contextual() {
    let go = emit_src(
        "\
fn main() {
    default := make(chan int, 1)
    select {
    v := <-default:
        fmt.println(\"{v}\")
    default:
        fmt.println(\"fallback\")
    }
}
",
    );
    assert!(
        go.contains("default_ := make(chan int, 1)"),
        "channel default must be mangled, got:\n{go}"
    );
    assert!(
        go.contains("case v := <-default_:"),
        "select receive from default must be mangled, got:\n{go}"
    );
    assert!(
        go.contains("\t\tdefault:\n"),
        "select default arm must remain Go keyword default:, got:\n{go}"
    );
}

#[test]
fn default_ident_for_range_and_iteration() {
    let go = emit_src(
        "\
fn main() {
    for default in 0..5 {
        fmt.println(\"{default}\")
    }
    nums := []int{1, 2, 3}
    for default in nums {
        fmt.println(\"{default}\")
    }
}
",
    );
    assert!(
        go.contains("for default_ := 0; default_ < 5; default_++ {"),
        "for-in range var default must be mangled, got:\n{go}"
    );
    assert!(
        go.contains("for _, default_ := range nums {"),
        "for-in slice iteration var default must be mangled, got:\n{go}"
    );
}
