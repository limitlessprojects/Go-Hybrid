use hy_emit::emit_go;
use hy_syntax::parse_file;
use std::process::Command;

fn emit(src: &str) -> String {
    let ast = parse_file("repro_4356.hy", src).expect("parse failed");
    emit_go(&ast).expect("emit_go failed")
}

#[test]
fn foreign_struct_literal_keyed_fields_normalize_to_go_names() {
    let go = emit(
        r#"
import "archive/tar"
import "net/http"
import "net/url"
type archive_hdr = tar.header
fn main() {
    h := &tar.header{name: "hello.txt", mode: 420, size: 5, typeflag: tar.type_reg}
    s := &http.server{addr: ":8080"}
    u := &url.url{scheme: "https", raw_query: "k=v"}
    r := &http.request{method: "GET", tls: nil}
    a := archive_hdr{name: "alias.txt", size: 1}
    _, _, _, _, _ = h, s, u, r, a
}
"#,
    );
    for needle in [
        "&tar.Header{Name: \"hello.txt\", Mode: 420, Size: 5, Typeflag: tar.TypeReg}",
        "&http.Server{Addr: \":8080\"}",
        "&url.URL{Scheme: \"https\", RawQuery: \"k=v\"}",
        "&http.Request{Method: \"GET\", TLS: nil}",
        "archive_hdr{Name: \"alias.txt\", Size: 1}",
    ] {
        assert!(go.contains(needle), "missing {needle} in:\n{go}");
    }
}

#[test]
fn struct_field_chained_foreign_ty_and_method_result_bindings() {
    let go = emit(
        r#"
import "archive/tar"
import "io"
import "log"
import "net/http"
import "time"
struct svc {
    lg *log.logger
    hdr *tar.header
    tr *tar.reader
    started time.time
}
fn run(r io.reader, req *http.request, s *svc) (string, error) {
    s.lg.println("ok")
    s.hdr.name = "b.txt"
    log.default().println("ok")
    _ = req.body.close()
    body := req.body
    _ = body.close()
    tr := tar.new_reader(r)
    hdr := tr.next()?
    hdr2 := s.tr.next()?
    t0 := time.now()
    if hdr.size >= 0 && t0.unix() + s.started.unix() + time.now().unix() >= 0 {
        return s.hdr.name + hdr.name + hdr2.name, nil
    }
    return "", nil
}
fn main() {}
"#,
    );
    for needle in [
        "s.lg.Println(\"ok\")",
        "s.hdr.Name = \"b.txt\"",
        "log.Default().Println(\"ok\")",
        "req.Body.Close()",
        "body.Close()",
        "tr.Next()",
        "s.tr.Next()",
        "if hdr.Size >= 0 && t0.Unix() + s.started.Unix() + time.Now().Unix() >= 0",
        "return s.hdr.Name + hdr.Name + hdr2.Name, nil",
    ] {
        assert!(go.contains(needle), "missing {needle} in:\n{go}");
    }
}

#[test]
fn user_struct_generic_holder_and_slice_pop_semantics() {
    let go = emit(
        r#"
import "archive/tar"
import "log"
import "net/http"
type logger_ptr = *log.logger
struct item_box { raw_name string }
struct holder[t any] { inner_val t }
fn exercise_generic(lg *log.logger, r *http.request) string {
    b := item_box{raw_name: "a"}
    b.raw_name = "b"
    g := holder[tar.header]{inner_val: tar.header{name: "x.txt"}}
    g.inner_val.name = "y.txt"
    w := holder[logger_ptr]{inner_val: lg}
    w.inner_val.println(g.inner_val.name)
    vals := r.header.values("Accept")
    encs := r.transfer_encoding
    _ = vals.pop()
    _ = encs.pop()
    opt := some(1)
    _ = opt.is_some()
    return b.raw_name + g.inner_val.name
}
fn main() {}
"#,
    );
    for needle in [
        "item_box{raw_name: \"a\"}",
        "b.raw_name = \"b\"",
        "holder[tar.Header]{inner_val: tar.Header{Name: \"x.txt\"}}",
        "g.inner_val.Name = \"y.txt\"",
        "w.inner_val.Println(g.inner_val.Name)",
        "hy_pop(&vals)",
        "hy_pop(&encs)",
        "opt.is_some()",
        "return b.raw_name + g.inner_val.Name",
    ] {
        assert!(go.contains(needle), "missing {needle} in:\n{go}");
    }
}

#[test]
fn emitted_foreign_surface_compiles_with_go_build() {
    let go = emit(
        r#"
import "archive/tar"
import "io"
import "log"
import "net/http"
import "net/url"
import "time"
type logger_ptr = *log.logger
struct holder[t any] { inner_val t }
struct svc {
    lg *log.logger
    hdr *tar.header
    started time.time
}
fn exercise(r io.reader, req *http.request, s *svc) (string, error) {
    h := &tar.header{name: "hello.txt", mode: 420, size: 5, typeflag: tar.type_reg}
    srv := &http.server{addr: ":8080"}
    u := &url.url{scheme: "https", raw_query: "k=v"}
    s.lg.println(srv.addr, u.raw_query, h.name)
    s.hdr.name = "b.txt"
    g := holder[tar.header]{inner_val: tar.header{name: "g.txt"}}
    g.inner_val.name = "g2.txt"
    w := holder[logger_ptr]{inner_val: s.lg}
    w.inner_val.println(g.inner_val.name)
    _ = req.body.close()
    vals := req.header.values("Accept")
    _ = vals.pop()
    log.default().println(s.hdr.name)
    tr := tar.new_reader(r)
    hdr := tr.next()?
    if hdr.size >= 0 && s.started.unix() >= 0 && time.now().unix() >= 0 {
        return hdr.name + g.inner_val.name, nil
    }
    return "", nil
}
fn main() {}
"#,
    );
    if Command::new("go").arg("version").output().is_err() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("hy_issue_4356_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir temp");
    let src_path = dir.join("main.go");
    let bin_path = dir.join("out_bin");
    std::fs::write(&src_path, &go).expect("write main.go");
    let out = Command::new("go")
        .args(["build", "-o"])
        .arg(&bin_path)
        .arg(&src_path)
        .output()
        .expect("go build exec");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        out.status.success(),
        "go build failed:\n{}\n{go}",
        String::from_utf8_lossy(&out.stderr)
    );
}
