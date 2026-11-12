use hy_emit::emit_go;
use hy_syntax::parse_file;
use std::process::Command;

fn assert_go_builds(case_name: &str, go: &str) {
    let tmp_dir = std::env::temp_dir();
    let file_path = tmp_dir.join(format!("test_foreign_struct_lit_{case_name}.go"));
    std::fs::write(&file_path, go.as_bytes()).expect("write go file");

    let out = Command::new("go")
        .args(["build", "-o", "/dev/null"])
        .arg(&file_path)
        .output()
        .expect("execute go build");

    let _ = std::fs::remove_file(&file_path);

    assert!(
        out.status.success(),
        "go build failed on emitted Go for {case_name}:\n---\n{go}\n---\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn test_foreign_struct_literal_keyed_fields_normalize_and_build() {
    let src = r#"package main
import "http"
import "tar"
import "url"

type srv_alias = http.server

struct local_header {
    file_name string
    raw_query string
}

fn main() {
    hdr := &tar.header{
        name: "hello.txt",
        mode: 420,
        size: 5,
        typeflag: tar.type_reg,
    }
    srv := &http.server{
        addr: ":8080",
    }
    aliased := &srv_alias{
        addr: ":9090",
    }
    u := &url.url{
        scheme: "https",
        host: "example.com",
        path: "/v1",
        raw_query: "k=v",
    }
    loc := local_header{
        file_name: "local.txt",
        raw_query: "x=1",
    }
    _ = hdr
    _ = srv
    _ = aliased
    _ = u
    _ = loc
}
"#;

    let ast = parse_file("test.hy", src).expect("must parse");
    let go = emit_go(&ast).expect("must emit");

    for expected in [
        "Name: \"hello.txt\"",
        "Mode: 420",
        "Size: 5",
        "Typeflag: tar.TypeReg",
        "Addr: \":8080\"",
        "Addr: \":9090\"",
        "Scheme: \"https\"",
        "Host: \"example.com\"",
        "Path: \"/v1\"",
        "RawQuery: \"k=v\"",
        "file_name: \"local.txt\"",
        "raw_query: \"x=1\"",
    ] {
        assert!(
            go.contains(expected),
            "expected emitted Go to contain `{expected}`, got:\n{go}"
        );
    }

    assert_go_builds("struct_lits", &go);
}

#[test]
fn test_user_struct_foreign_field_and_place_access_normalize_and_build() {
    let src = r#"package main
import "log"
import "os"
import "tar"

struct service {
    lg *log.logger
    hdr tar.header
}

struct holder[t] {
    lg *log.logger
    item t
}

fn run(s *service) {
    s.lg.println("service ready")
    _ = s.lg.prefix()
    s.hdr.name = "updated.txt"
    s.hdr.size = 42
    _ = s.hdr.name
}

fn main() {
    lg := log.new(os.stderr, "[svc] ", log.lstdflags)
    s := service{
        lg: lg,
        hdr: tar.header{name: "init.txt", size: 1},
    }
    run(&s)
    h := holder[int]{lg: lg, item: 7}
    h.lg.println("holder ready")
    _ = h.item
}
"#;

    let ast = parse_file("test.hy", src).expect("must parse");
    let go = emit_go(&ast).expect("must emit");

    for expected in [
        "s.lg.Println(\"service ready\")",
        "s.lg.Prefix()",
        "s.hdr.Name = \"updated.txt\"",
        "s.hdr.Size = 42",
        "_ = s.hdr.Name",
        "h.lg.Println(\"holder ready\")",
    ] {
        assert!(
            go.contains(expected),
            "expected emitted Go to contain `{expected}`, got:\n{go}"
        );
    }

    assert_go_builds("struct_fields", &go);
}

#[test]
fn test_chained_foreign_calls_and_foreign_ty_fallback_methods_normalize_and_build() {
    let src = r#"package main
import "log"
import "os"
import "tar"
import "time"

fn inspect_archive() error {
    tr := tar.new_reader(os.stdin)
    hdr := tr.next()?
    _ = hdr.name
    _ = hdr.size
    return nil
}

fn main() {
    log.default().println("chained call")
    _ = log.default().flags()
    t0 := time.now()
    _ = t0.unix()
    _ = t0.unix_nano()
    _ = inspect_archive()
}
"#;

    let ast = parse_file("test.hy", src).expect("must parse");
    let go = emit_go(&ast).expect("must emit");

    for expected in [
        "log.Default().Println(\"chained call\")",
        "log.Default().Flags()",
        "t0.Unix()",
        "t0.UnixNano()",
        "hdr.Name",
        "hdr.Size",
    ] {
        assert!(
            go.contains(expected),
            "expected emitted Go to contain `{expected}`, got:\n{go}"
        );
    }

    assert_go_builds("chained_calls", &go);
}
