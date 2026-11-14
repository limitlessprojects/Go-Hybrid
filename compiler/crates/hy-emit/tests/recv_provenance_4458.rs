use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("parses");
    emit_go(&ast).unwrap_or_else(|e| panic!("emit failed: {}", e.message))
}

#[test]
fn short_decl_and_comma_ok_receive_normalize_foreign_members() {
    let go = emit(
        "import \"fmt\"\nimport \"url\"\nfn main() {\n    ch := make(chan url.url, 3)\n    got := <-ch\n    fmt.println(got.host)\n    got2, ok := <-ch\n    if ok {\n        fmt.println(got2.host, got2.request_uri())\n    }\n    if got3, ok3 := <-ch; ok3 {\n        fmt.println(got3.host)\n    }\n}\n",
    );
    for want in [
        "got.Host",
        "got2.Host",
        "got2.RequestURI()",
        "got3.Host",
    ] {
        assert!(go.contains(want), "missing `{want}` in:\n{go}");
    }
    for bad in ["got.host", "got2.host", "got2.request_uri()", "got3.host"] {
        assert!(!go.contains(bad), "unexpected snake_case `{bad}` in:\n{go}");
    }
}

#[test]
fn pointer_foreign_and_type_alias_channel_receive_normalize_methods() {
    let go = emit(
        "import \"atomic\"\nimport \"fmt\"\ntype counter_ptr = *atomic.int64\nfn work(ch chan *atomic.int64, ch_alias chan counter_ptr) {\n    got := <-ch\n    got.store(5)\n    fmt.println(got.add(2))\n    for p in ch_alias {\n        p.store(9)\n    }\n}\nfn main() {\n    _ = work\n}\n",
    );
    for want in [
        "got.Store(5)",
        "got.Add(2)",
        "for p := range ch_alias {",
        "p.Store(9)",
    ] {
        assert!(go.contains(want), "missing `{want}` in:\n{go}");
    }
    for bad in ["got.store", "got.add", "p.store", "for _, p := range"] {
        assert!(!go.contains(bad), "unexpected `{bad}` in:\n{go}");
    }
}

#[test]
fn select_arms_and_for_iterate_normalize_foreign_and_user_struct_members() {
    let go = emit(
        "import \"atomic\"\nimport \"fmt\"\nimport \"sync\"\nimport \"url\"\nstruct box {\n    mu sync.mutex\n}\nfn drive(u_ch <-chan url.url, a_ch <-chan *atomic.int64, b_ch chan box) {\n    select {\n        v1 := <-u_ch: fmt.println(v1.host)\n        v2, ok := <-a_ch: if ok {\n            v2.store(1)\n        }\n    }\n    for u in u_ch {\n        fmt.println(u.host)\n    }\n    for b in b_ch {\n        b.mu.lock()\n        b.mu.unlock()\n    }\n}\nfn main() {\n    _ = drive\n}\n",
    );
    for want in [
        "v1.Host",
        "v2.Store(1)",
        "for u := range u_ch {",
        "u.Host",
        "for b := range b_ch {",
        "b.mu.Lock()",
        "b.mu.Unlock()",
    ] {
        assert!(go.contains(want), "missing `{want}` in:\n{go}");
    }
    for bad in ["v1.host", "v2.store", "u.host", "b.mu.lock", "b.mu.unlock"] {
        assert!(!go.contains(bad), "unexpected `{bad}` in:\n{go}");
    }
}

#[test]
fn direct_receive_selector_struct_field_chan_fn_return_and_nested_chan() {
    let go = emit(
        "import \"atomic\"\nimport \"bytes\"\nimport \"fmt\"\nimport \"sync\"\nimport \"url\"\nstruct box {\n    mu sync.mutex\n}\nstruct hub {\n    urls chan url.url\n}\nstruct stack {\n    n int\n}\nfn (s *stack) pop() int {\n    return s.n\n}\nfn make_url_ch() chan url.url {\n    return make(chan url.url, 1)\n}\nfn exercise(u_ch chan url.url, a_ch chan *atomic.int64, b_ch chan box, buf_ch chan bytes.buffer, pop_ch chan stack, h hub) {\n    fmt.println((<-u_ch).host)\n    (<-a_ch).store(7)\n    (<-b_ch).mu.lock()\n    uh := <-h.urls\n    fmt.println(uh.host)\n    uf := <-make_url_ch()\n    fmt.println(uf.host)\n    alias_ch := u_ch\n    ua := <-alias_ch\n    fmt.println(ua.host)\n    outer := make(chan chan url.url, 1)\n    inner := <-outer\n    un := <-inner\n    fmt.println(un.host)\n    u := u_ch\n    u := <-u\n    fmt.println(u.host)\n    buf := <-buf_ch\n    buf.write_string(\"hi\")\n    (<-buf_ch).write_string(\"yo\")\n    fmt.println((<-pop_ch).pop())\n}\nfn main() {\n    _ = exercise\n}\n",
    );
    for want in [
        "(<-u_ch).Host",
        "(<-a_ch).Store(7)",
        "(<-b_ch).mu.Lock()",
        "uh.Host",
        "uf.Host",
        "ua.Host",
        "un.Host",
        "u.Host",
        "buf.WriteString(\"hi\")",
        "(<-buf_ch).WriteString(\"yo\")",
        "(<-pop_ch).pop()",
    ] {
        assert!(go.contains(want), "missing `{want}` in:\n{go}");
    }
    assert!(
        !go.contains("hy_pop"),
        "user struct method `(<-pop_ch).pop()` must not rewrite to hy_pop:\n{go}"
    );
}

#[test]
fn emitted_recv_provenance_programs_compile_and_run_with_go() {
    let go = emit(
        "import \"atomic\"\nimport \"fmt\"\nimport \"sync\"\nimport \"url\"\nstruct box {\n    mu sync.mutex\n    n int\n}\nfn make_url_ch() chan *url.url {\n    return make(chan *url.url, 2)\n}\nfn main() {\n    u, _ := url.parse(\"https://example.org/ok\")\n    u_ch := make_url_ch()\n    u_ch <- u\n    u_ch <- u\n    close(u_ch)\n    got := <-u_ch\n    fmt.println(got.host)\n    for rest in u_ch {\n        fmt.println(rest.host)\n    }\n    var a atomic.int64\n    a_ch := make(chan *atomic.int64, 1)\n    a_ch <- &a\n    if ap, ok := <-a_ch; ok {\n        ap.store(42)\n    }\n    fmt.println(a.load())\n    b_ch := make(chan box, 1)\n    b_ch <- box{n: 7}\n    select {\n        b := <-b_ch: {\n            b.mu.lock()\n            fmt.println(b.n)\n            b.mu.unlock()\n        }\n    }\n}\n",
    );
    let go_bin = std::process::Command::new("go")
        .arg("version")
        .output()
        .is_ok_and(|o| o.status.success())
        .then(|| "go".to_string())
        .or_else(|| {
            let p = "/home/max/.local/go/bin/go";
            std::path::Path::new(p).exists().then(|| p.to_string())
        });
    let Some(go_bin) = go_bin else {
        return;
    };
    let tmp = std::env::temp_dir().join(format!("hy_recv_provenance_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("create temp dir");
    std::fs::write(tmp.join("go.mod"), "module recvprov4458\n\ngo 1.21\n").expect("write go.mod");
    std::fs::write(tmp.join("main.go"), &go).expect("write main.go");
    let out = std::process::Command::new(go_bin)
        .args(["run", "."])
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
    assert_eq!(stdout, "example.org\nexample.org\n42\n7\n");
}
