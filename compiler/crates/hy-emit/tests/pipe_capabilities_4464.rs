//! Integration tests for linear SPSC channel capabilities and pipe lowering (issue #4464).
//!
//! Verifies:
//! 1) Exact #4464 reproducer (cap.hy) accepts in hy-check, emits compilable Go with zero "hy" imports,
//!    builds with go build, and outputs "0\n1\n2\n".
//! 2) All accepted patterns from channel_liveness_4200.rs: P01, P07, P08, P10, P12, P13, P15, P16, P17, P20.
//! 3) Pre-declared var tx/rx assignment, buffered pipes, various element types (u8, []u8, user struct),
//!    type aliases, package mode (emit_go_package), unhijacked local hy shadow, witness proof generation.
//! 4) Cold CLI test running hyb check and hyb build on cap.hy.

use hy_check::{check_file, Verdict};
use hy_emit::{emit_go, emit_go_package, emit_go_with_proof};
use hy_syntax::parse_file;
use std::path::{Path, PathBuf};
use std::process::Command;

fn run_go_build_only(name: &str, go_src: &str) {
    let tmp_dir = std::env::temp_dir();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let go_file = tmp_dir.join(format!("test_pipe_build_{}_{}.go", name, nonce));
    let bin_file = tmp_dir.join(format!(
        "test_pipe_build_{}_{}{}",
        name,
        nonce,
        if cfg!(windows) { ".exe" } else { "" }
    ));

    std::fs::write(&go_file, go_src.as_bytes()).expect("write Go file");

    let build_out = Command::new("go")
        .args(["build", "-o"])
        .arg(&bin_file)
        .arg(&go_file)
        .output()
        .expect("invoke go build");

    let _ = std::fs::remove_file(&go_file);
    if bin_file.exists() {
        let _ = std::fs::remove_file(&bin_file);
    }

    assert!(
        build_out.status.success(),
        "go build failed for test '{}':\n---\n{}\n---\nstderr:\n{}",
        name,
        go_src,
        String::from_utf8_lossy(&build_out.stderr)
    );
}

fn run_go_build_and_exec(name: &str, go_src: &str, expected_stdout: Option<&str>) {
    let tmp_dir = std::env::temp_dir();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let go_file = tmp_dir.join(format!("test_pipe_4464_{}_{}.go", name, nonce));
    let bin_file = tmp_dir.join(format!(
        "test_pipe_4464_{}_{}{}",
        name,
        nonce,
        if cfg!(windows) { ".exe" } else { "" }
    ));

    std::fs::write(&go_file, go_src.as_bytes()).expect("write Go file");

    let build_out = Command::new("go")
        .args(["build", "-o"])
        .arg(&bin_file)
        .arg(&go_file)
        .output()
        .expect("invoke go build");

    let _ = std::fs::remove_file(&go_file);

    assert!(
        build_out.status.success(),
        "go build failed for test '{}':\n---\n{}\n---\nstderr:\n{}",
        name,
        go_src,
        String::from_utf8_lossy(&build_out.stderr)
    );

    let run_out = Command::new(&bin_file).output().expect("run compiled binary");
    let _ = std::fs::remove_file(&bin_file);

    assert!(
        run_out.status.success(),
        "binary execution failed for test '{}': stderr:\n{}",
        name,
        String::from_utf8_lossy(&run_out.stderr)
    );

    if let Some(expected) = expected_stdout {
        let actual = String::from_utf8_lossy(&run_out.stdout).replace("\r\n", "\n");
        assert_eq!(
            actual, expected,
            "stdout mismatch for test '{}':\nexpected:\n{:?}\nactual:\n{:?}",
            name, expected, actual
        );
    }
}

fn find_hyb_bin() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("CARGO_BIN_EXE_hyb") {
        let pb = PathBuf::from(p);
        if pb.exists() {
            return Some(pb);
        }
    }
    // Search up from current executable
    if let Ok(mut dir) = std::env::current_exe() {
        while dir.pop() {
            let candidate = dir.join(if cfg!(windows) { "hyb.exe" } else { "hyb" });
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    let manifest_target = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| {
            p.join("target")
                .join("debug")
                .join(if cfg!(windows) { "hyb.exe" } else { "hyb" })
        });
    if let Some(target) = manifest_target {
        if target.exists() {
            return Some(target);
        }
    }
    None
}

#[test]
fn test_01_exact_4464_reproducer_executes_cleanly() {
    let src = r#"
fn worker(tx hy.send_cap[int]) {
    for i in 0..3 {
        tx.send(i)
    }
    tx.close()
}

fn main() {
    tx, rx := hy.pipe[int]()
    task worker(move tx)
    for v in rx {
        fmt.println(v)
    }
}
"#;
    let file = parse_file("cap.hy", src).expect("parse cap.hy");
    let check_res = check_file(&file);
    assert_eq!(
        check_res.verdict,
        Verdict::Accept,
        "cap.hy must accept in hy-check: {:?}",
        check_res.diagnostics
    );

    let go = emit_go(&file).expect("emit cap.hy");
    assert!(
        !go.contains("import \"hy\"") && !go.contains("\t\"hy\"\n"),
        "must not import invalid 'hy' package:\n{}",
        go
    );
    assert!(
        go.contains("type hy_send_cap[t any] chan<- t"),
        "must include PIPE_PRELUDE send_cap:\n{}",
        go
    );
    assert!(
        go.contains("type hy_recv_cap[t any] <-chan t"),
        "must include PIPE_PRELUDE recv_cap:\n{}",
        go
    );
    assert!(
        go.contains("func hy_pipe[t any]"),
        "must include PIPE_PRELUDE hy_pipe:\n{}",
        go
    );
    assert!(
        go.contains("for v := range rx"),
        "must emit channel range for v := range rx:\n{}",
        go
    );

    run_go_build_and_exec("exact_4464", &go, Some("0\n1\n2\n"));
}

#[test]
fn test_02_p01_happy_path_recv_and_drain() {
    let src = r#"
fn worker(tx hy.send_cap[int]) {
    tx.send(42)
    tx.close()
}

fn main() {
    tx, rx := hy.pipe[int]()
    task worker(move tx)
    v := rx.recv()
    _ = v
    rx.drain()
}
"#;
    let file = parse_file("p01.hy", src).expect("parse p01.hy");
    let check_res = check_file(&file);
    assert_eq!(check_res.verdict, Verdict::Accept);
    let go = emit_go(&file).expect("emit p01");
    assert!(!go.contains("\"hy\""));
    assert!(go.contains("rx.recv()"));
    assert!(go.contains("rx.drain()"));
    run_go_build_and_exec("p01", &go, None);
}

#[test]
fn test_03_p07_multi_stage_pipeline() {
    let src = r#"
fn stage1(tx hy.send_cap[int]) {
    tx.send(1)
    tx.close()
}

fn stage2(rx hy.recv_cap[int], tx hy.send_cap[int]) {
    v := rx.recv()
    tx.send(v)
    rx.drain()
    tx.close()
}

fn main() {
    tx1, rx1 := hy.pipe[int]()
    tx2, rx2 := hy.pipe[int]()
    task stage1(move tx1)
    task stage2(move rx1, move tx2)
    res := rx2.recv()
    _ = res
    rx2.drain()
}
"#;
    let file = parse_file("p07.hy", src).expect("parse p07");
    assert_eq!(check_file(&file).verdict, Verdict::Accept);
    let go = emit_go(&file).expect("emit p07");
    assert!(!go.contains("\"hy\""));
    run_go_build_and_exec("p07", &go, None);
}

#[test]
fn test_04_p08_loop_send_unread_range() {
    let src = r#"
fn worker(tx hy.send_cap[int]) {
    for i in 0..5 {
        tx.send(i)
    }
    tx.close()
}

fn main() {
    tx, rx := hy.pipe[int]()
    task worker(move tx)
    for v in rx {
    }
}
"#;
    let file = parse_file("p08.hy", src).expect("parse p08");
    assert_eq!(check_file(&file).verdict, Verdict::Accept);
    let go = emit_go(&file).expect("emit p08");
    assert!(!go.contains("\"hy\""));
    assert!(go.contains("for range rx"));
    run_go_build_and_exec("p08", &go, None);
}

#[test]
fn test_05_p10_select_receive_with_drain() {
    let src = r#"
fn worker(tx hy.send_cap[int]) {
    tx.send(100)
    tx.close()
}

fn main() {
    tx, rx := hy.pipe[int]()
    task worker(move tx)
    select {
        <-rx => {
            rx.drain()
        }
        default => {
            rx.drain()
        }
    }
}
"#;
    let file = parse_file("p10.hy", src).expect("parse p10");
    assert_eq!(check_file(&file).verdict, Verdict::Accept);
    let go = emit_go(&file).expect("emit p10");
    assert!(!go.contains("\"hy\""));
    assert!(go.contains("case <-rx:"));
    run_go_build_and_exec("p10", &go, None);
}

#[test]
fn test_06_p12_move_rx_to_consumer() {
    let src = r#"
fn consumer(rx hy.recv_cap[int]) {
    v := rx.recv()
    _ = v
    rx.drain()
}

fn main() {
    tx, rx := hy.pipe[int]()
    task consumer(move rx)
    tx.send(99)
    tx.close()
}
"#;
    let file = parse_file("p12.hy", src).expect("parse p12");
    assert_eq!(check_file(&file).verdict, Verdict::Accept);
    let go = emit_go(&file).expect("emit p12");
    assert!(!go.contains("\"hy\""));
    run_go_build_and_exec("p12", &go, None);
}

#[test]
fn test_07_p13_for_range_drain_in_main() {
    let src = r#"
fn main() {
    tx, rx := hy.pipe[int]()
    tx.send(1)
    tx.close()
    for v in rx {
    }
}
"#;
    let file = parse_file("p13.hy", src).expect("parse p13");
    assert_eq!(check_file(&file).verdict, Verdict::Accept);
    let go = emit_go(&file).expect("emit p13");
    assert!(!go.contains("\"hy\""));
    run_go_build_only("p13", &go);
}

#[test]
fn test_08_p15_explicit_drain() {
    let src = r#"
fn worker(tx hy.send_cap[int]) {
    tx.send(5)
    tx.close()
}

fn main() {
    tx, rx := hy.pipe[int]()
    task worker(move tx)
    rx.drain()
}
"#;
    let file = parse_file("p15.hy", src).expect("parse p15");
    assert_eq!(check_file(&file).verdict, Verdict::Accept);
    let go = emit_go(&file).expect("emit p15");
    assert!(!go.contains("\"hy\""));
    run_go_build_and_exec("p15", &go, None);
}

#[test]
fn test_09_p16_bidirectional_pipes() {
    let src = r#"
fn node_a(tx hy.send_cap[int], rx hy.recv_cap[int]) {
    tx.send(1)
    tx.close()
    rx.drain()
}

fn main() {
    tx1, rx1 := hy.pipe[int]()
    tx2, rx2 := hy.pipe[int]()
    task node_a(move tx1, move rx2)
    tx2.send(2)
    tx2.close()
    rx1.drain()
}
"#;
    let file = parse_file("p16.hy", src).expect("parse p16");
    assert_eq!(check_file(&file).verdict, Verdict::Accept);
    let go = emit_go(&file).expect("emit p16");
    assert!(!go.contains("\"hy\""));
    run_go_build_only("p16", &go);
}

#[test]
fn test_10_p17_nested_scope() {
    let src = r#"
fn main() {
    {
        tx, rx := hy.pipe[int]()
        tx.send(1)
        tx.close()
        rx.drain()
    }
}
"#;
    let file = parse_file("p17.hy", src).expect("parse p17");
    assert_eq!(check_file(&file).verdict, Verdict::Accept);
    let go = emit_go(&file).expect("emit p17");
    assert!(!go.contains("\"hy\""));
    run_go_build_only("p17", &go);
}

#[test]
fn test_11_p20_multiple_independent_pipes() {
    let src = r#"
fn worker1(tx hy.send_cap[int]) {
    tx.send(1)
    tx.close()
}

fn worker2(tx hy.send_cap[int]) {
    tx.send(2)
    tx.close()
}

fn main() {
    tx1, rx1 := hy.pipe[int]()
    tx2, rx2 := hy.pipe[int]()
    task worker1(move tx1)
    task worker2(move tx2)
    rx1.drain()
    rx2.drain()
}
"#;
    let file = parse_file("p20.hy", src).expect("parse p20");
    assert_eq!(check_file(&file).verdict, Verdict::Accept);
    let go = emit_go(&file).expect("emit p20");
    assert!(!go.contains("\"hy\""));
    run_go_build_and_exec("p20", &go, None);
}

#[test]
fn test_12_predeclared_var_pipe_assignment() {
    let src = r#"
fn main() {
    var tx hy.send_cap[int]
    var rx hy.recv_cap[int]
    tx, rx = hy.pipe[int]()
    tx.send(42)
    tx.close()
    rx.drain()
}
"#;
    let file = parse_file("predeclared.hy", src).expect("parse predeclared");
    assert_eq!(check_file(&file).verdict, Verdict::Accept);
    let go = emit_go(&file).expect("emit predeclared");
    assert!(!go.contains("\"hy\""));
    assert!(go.contains("var tx hy_send_cap[int]"));
    assert!(go.contains("var rx hy_recv_cap[int]"));
    assert!(go.contains("tx, rx = hy_pipe[int]()"));
    run_go_build_only("predeclared", &go);
}

#[test]
fn test_13_buffered_pipe_with_capacity() {
    let src = r#"
fn main() {
    tx, rx := hy.pipe[int](4)
    tx.send(1)
    tx.send(2)
    tx.close()
    for v in rx {
        _ = v
    }
}
"#;
    let file = parse_file("buffered.hy", src).expect("parse buffered");
    assert_eq!(check_file(&file).verdict, Verdict::Accept);
    let go = emit_go(&file).expect("emit buffered");
    assert!(!go.contains("\"hy\""));
    assert!(go.contains("hy_pipe[int](4)"));
    run_go_build_and_exec("buffered", &go, None);
}

#[test]
fn test_14_element_types_primitive_slice_and_struct() {
    let src = r#"
struct item {
    id int
}

fn main() {
    tx1, rx1 := hy.pipe[u8]()
    tx1.send(u8(255))
    tx1.close()
    rx1.drain()

    tx2, rx2 := hy.pipe[[]u8]()
    tx2.send([]u8{1, 2, 3})
    tx2.close()
    rx2.drain()

    tx3, rx3 := hy.pipe[item]()
    tx3.send(item{id: 7})
    tx3.close()
    it := rx3.recv()
    _ = it.id
    rx3.drain()
}
"#;
    let file = parse_file("elem_types.hy", src).expect("parse elem_types");
    assert_eq!(check_file(&file).verdict, Verdict::Accept);
    let go = emit_go(&file).expect("emit elem_types");
    assert!(!go.contains("\"hy\""));
    assert!(go.contains("hy_pipe[uint8]()"));
    assert!(go.contains("hy_pipe[[]uint8]()"));
    assert!(go.contains("hy_pipe[item]()"));
    run_go_build_only("elem_types", &go);
}

#[test]
fn test_15_type_alias_for_capability() {
    let src = r#"
type int_rx = hy.recv_cap[int]

fn consume(rx int_rx) {
    rx.drain()
}

fn main() {
    tx, rx := hy.pipe[int]()
    tx.close()
    task consume(move rx)
}
"#;
    let file = parse_file("type_alias.hy", src).expect("parse type_alias");
    let go = emit_go(&file).expect("emit type_alias");
    assert!(!go.contains("\"hy\""));
    assert!(go.contains("type int_rx = hy_recv_cap[int]"));
    run_go_build_and_exec("type_alias", &go, None);
}

#[test]
fn test_16_package_mode_emit_go_package() {
    let src = r#"
pub fn make_pipe() (hy.send_cap[int], hy.recv_cap[int]) {
    tx, rx := hy.pipe[int]()
    return tx, rx
}
"#;
    let file = parse_file("pkg_pipe.hy", src).expect("parse pkg_pipe");
    let go = emit_go_package(&file, "pipepkg", "hybrid.program").expect("emit_go_package");
    assert!(go.starts_with("package pipepkg\n"), "must be package pipepkg:\n{}", go);
    assert!(!go.contains("\"hy\""));
    assert!(go.contains("func MakePipe() (hy_send_cap[int], hy_recv_cap[int])"));
    assert!(go.contains("PIPE_PRELUDE") || go.contains("type hy_send_cap[t any] chan<- t"));
}

#[test]
fn test_17_shadowed_local_hy_not_hijacked() {
    let src = r#"
struct hy_mgr {
    ready bool
}

fn main() {
    hy := hy_mgr{ready: true}
    _ = hy.ready
}
"#;
    let file = parse_file("shadow.hy", src).expect("parse shadow");
    let go = emit_go(&file).expect("emit shadow");
    assert!(!go.contains("hy_pipe"));
    assert!(go.contains("hy := hy_mgr{ready: true}"));
    run_go_build_and_exec("shadow", &go, None);
}

#[test]
fn test_18_emit_go_with_proof_records_task_and_send_witnesses() {
    // 18a: tx.send(v) capability method send witnesses as a send site
    let src1 = r#"
fn worker(tx hy.send_cap[int]) {
    tx.send(1)
    tx.close()
}

fn main() {
    tx, rx := hy.pipe[int]()
    task worker(move tx)
    rx.drain()
}
"#;
    let file1 = parse_file("proof1.hy", src1).expect("parse proof1");
    let (go1, proof_json1) = emit_go_with_proof(&file1).expect("emit with proof");
    assert!(!go1.contains("\"hy\""));
    assert!(proof_json1.contains("\"task_sites\": 1") || proof_json1.contains("\"task_sites\":1"));
    assert!(proof_json1.contains("\"send_sites\": 1") || proof_json1.contains("\"send_sites\":1"));

    // 18b: Both tx.send(1) and tx <- 2 record send sites (total 2)
    let src2 = r#"
fn worker(tx hy.send_cap[int]) {
    tx.send(1)
    tx <- 2
    tx.close()
}

fn main() {
    tx, rx := hy.pipe[int]()
    task worker(move tx)
    rx.drain()
}
"#;
    let file2 = parse_file("proof2.hy", src2).expect("parse proof2");
    let (go2, proof_json2) = emit_go_with_proof(&file2).expect("emit with proof 2");
    assert!(!go2.contains("\"hy\""));
    assert!(proof_json2.contains("\"task_sites\": 1") || proof_json2.contains("\"task_sites\":1"));
    assert!(proof_json2.contains("\"send_sites\": 2") || proof_json2.contains("\"send_sites\":2"));
}

#[test]
fn test_19_cold_cli_hyb_check_and_build() {
    let Some(hyb) = find_hyb_bin() else {
        eprintln!("skipping test_19: hyb binary not found in target dir (run cargo build --bin hyb)");
        return;
    };

    let tmp_dir = std::env::temp_dir();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let hy_file = tmp_dir.join(format!("test_cap_repro_{}.hy", nonce));
    let bin_file = tmp_dir.join(format!(
        "test_cap_repro_{}{}",
        nonce,
        if cfg!(windows) { ".exe" } else { "" }
    ));

    let src = r#"
fn worker(tx hy.send_cap[int]) {
    for i in 0..3 {
        tx.send(i)
    }
    tx.close()
}

fn main() {
    tx, rx := hy.pipe[int]()
    task worker(move tx)
    for v in rx {
        fmt.println(v)
    }
}
"#;
    std::fs::write(&hy_file, src.as_bytes()).expect("write test hy file");

    let mut check_cmd = Command::new(&hyb);
    check_cmd
        .arg("check")
        .arg(&hy_file)
        .env("HYB_ALLOW_TOOLCHAIN_DRIFT", "1");
    if let Ok(p) = std::env::var("PATH") {
        check_cmd.env("PATH", p);
    }
    let check_out = check_cmd.output().expect("invoke hyb check");
    assert!(
        check_out.status.success(),
        "hyb check failed on cap.hy:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&check_out.stdout),
        String::from_utf8_lossy(&check_out.stderr)
    );

    let mut build_cmd = Command::new(&hyb);
    build_cmd
        .arg("build")
        .arg(&hy_file)
        .arg("--emit-proof")
        .arg("-o")
        .arg(&bin_file)
        .env("HYB_ALLOW_TOOLCHAIN_DRIFT", "1");
    if let Ok(p) = std::env::var("PATH") {
        build_cmd.env("PATH", p);
    }
    let build_out = build_cmd.output().expect("invoke hyb build");
    let _ = std::fs::remove_file(&hy_file);

    assert!(
        build_out.status.success(),
        "hyb build failed on cap.hy:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build_out.stdout),
        String::from_utf8_lossy(&build_out.stderr)
    );

    let run_out = Command::new(&bin_file).output().expect("run built binary");
    let _ = std::fs::remove_file(&bin_file);

    assert!(
        run_out.status.success(),
        "running built binary failed: stderr:\n{}",
        String::from_utf8_lossy(&run_out.stderr)
    );

    let actual = String::from_utf8_lossy(&run_out.stdout).replace("\r\n", "\n");
    assert_eq!(actual, "0\n1\n2\n", "CLI repro output mismatch");
}
