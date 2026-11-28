use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("parses");
    emit_go(&ast).unwrap_or_else(|e| panic!("emit failed: {}", e.message))
}

fn find_go() -> Option<String> {
    if std::process::Command::new("go")
        .arg("version")
        .output()
        .is_ok_and(|o| o.status.success())
    {
        return Some("go".to_string());
    }
    let fallback = "/home/max/.local/go/bin/go";
    std::path::Path::new(fallback)
        .exists()
        .then(|| fallback.to_string())
}

#[test]
fn pipe_happy_path_and_explicit_hy_import_emit_prelude_without_hy_import() {
    let go = emit(
        "import \"hy\"\nfn worker(tx hy.send_cap[int]) {\n    tx.send(42)\n    tx.close()\n}\nfn main() {\n    tx, rx := hy.pipe[int]()\n    task worker(move tx)\n    v := rx.recv()\n    _ = v\n    rx.drain()\n}\n",
    );
    assert!(
        !go.contains("\"hy\""),
        "must not emit bogus Go import \"hy\":\n{go}"
    );
    for want in [
        "type hy_send_cap[t any] chan<- t",
        "type hy_recv_cap[t any] <-chan t",
        "func hy_pipe[t any](cap ...int) (hy_send_cap[t], hy_recv_cap[t])",
        "func worker(tx hy_send_cap[int])",
        "tx.send(42)",
        "tx.close()",
        "tx, rx := hy_pipe[int]()",
        "v := rx.recv()",
        "rx.drain()",
    ] {
        assert!(go.contains(want), "missing `{want}` in:\n{go}");
    }
    for bad in ["tx.Send", "tx.Close", "rx.Recv", "rx.Drain", "hy.pipe"] {
        assert!(!go.contains(bad), "unexpected `{bad}` in:\n{go}");
    }
}

#[test]
fn new_pipe_for_range_select_closure_and_typed_var_emit_correctly() {
    let go = emit(
        "import \"fmt\"\nstruct pair {\n    tx hy.send_cap[string]\n    rx hy.recv_cap[string]\n}\nfn pipeline(rx1 hy.recv_cap[int], tx2 hy.send_cap[int]) {\n    v := rx1.recv()\n    tx2.send(v)\n    rx1.drain()\n    tx2.close()\n}\nfn main() {\n    tx, rx := hy.new_pipe[int](16)\n    tx.send(1)\n    tx.close()\n    for item in rx {\n        fmt.println(item)\n    }\n    var s hy.send_cap[int]\n    var r hy.recv_cap[int]\n    s, r = hy.pipe[int]()\n    go_fn := fn(sc hy.send_cap[int], rc hy.recv_cap[int]) {\n        sc.send(7)\n        sc.close()\n        select {\n            x := <-rc: fmt.println(x)\n        }\n        for rem in rc {\n            fmt.println(rem)\n        }\n    }\n    go_fn(s, r)\n    _ = pipeline\n}\n",
    );
    for want in [
        "tx, rx := hy_pipe[int](16)",
        "for item := range rx {",
        "var s hy_send_cap[int]",
        "var r hy_recv_cap[int]",
        "s, r = hy_pipe[int]()",
        "func(sc hy_send_cap[int], rc hy_recv_cap[int])",
        "sc.send(7)",
        "sc.close()",
        "case x := <-rc:",
        "for rem := range rc {",
        "rx hy_recv_cap[string]",
        "tx hy_send_cap[string]",
    ] {
        assert!(go.contains(want), "missing `{want}` in:\n{go}");
    }
    assert!(
        !go.contains("for _, item := range rx") && !go.contains("for _, rem := range rc"),
        "channel cap range must emit 1-var range:\n{go}"
    );
    assert!(
        !go.contains("sc.Send") && !go.contains("sc.Close"),
        "closure send_cap methods must stay lowercase:\n{go}"
    );
}

#[test]
fn non_pipe_programs_do_not_emit_pipe_prelude() {
    let go = emit("import \"fmt\"\nfn main() {\n    fmt.println(1)\n}\n");
    assert!(
        !go.contains("hy_pipe") && !go.contains("hy_send_cap") && !go.contains("hy_recv_cap"),
        "unexpected pipe prelude in plain program:\n{go}"
    );
}

#[test]
fn pipe_capabilities_build_and_run_cleanly_under_go_race() {
    let go = emit(
        "import \"fmt\"\nfn producer(sender hy.send_cap[int]) {\n    for i in 0..4 {\n        sender.send(i)\n    }\n    sender.close()\n}\nfn relay(upstream hy.recv_cap[int], downstream hy.send_cap[int]) {\n    first := upstream.recv()\n    downstream.send(first)\n    for rest in upstream {\n        downstream.send(rest)\n    }\n    downstream.close()\n}\nfn main() {\n    tx1, rx1 := hy.pipe[int]()\n    tx2, rx2 := hy.new_pipe[int](8)\n    task producer(move tx1)\n    task relay(move rx1, move tx2)\n    sum := 0\n    for v in rx2 {\n        sum = sum + v\n    }\n    tx3, rx3 := hy.pipe[string](2)\n    tx3.send(\"ok\")\n    tx3.close()\n    msg := \"\"\n    select {\n        s := <-rx3: msg = s\n    }\n    rx3.drain()\n    fmt.println(sum, msg)\n}\n",
    );
    let Some(go_bin) = find_go() else {
        return;
    };
    let tmp = std::env::temp_dir().join(format!("hy_pipe_caps_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("create temp dir");
    std::fs::write(tmp.join("go.mod"), "module pipecaps4464\n\ngo 1.21\n").expect("write go.mod");
    std::fs::write(tmp.join("main.go"), &go).expect("write main.go");
    let out = std::process::Command::new(&go_bin)
        .args(["run", "-race", "."])
        .current_dir(&tmp)
        .output()
        .expect("invoke go run -race");
    let _ = std::fs::remove_dir_all(&tmp);
    assert!(
        out.status.success(),
        "go run -race failed on emitted Go:\n{}\nstderr:\n{}",
        go,
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout, "6 ok\n");
}
