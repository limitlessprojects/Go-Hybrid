use hy_emit::emit_go;
use hy_syntax::parse_file;
use std::process::Command;

#[test]
fn test_curated_import_paths_normalized_to_std_paths() {
    let cases = [
        ("import \"http\"\nfn main() {\n    _ = http.status_ok\n}\n", "\"net/http\""),
        ("import \"httptest\"\nfn main() {\n    _ = httptest.new_recorder()\n}\n", "\"net/http/httptest\""),
        ("import \"url\"\nfn main() {\n    _ = url.query_escape(\"x\")\n}\n", "\"net/url\""),
        ("import \"json\"\nfn main() {\n    _ = json.marshal(1)\n}\n", "\"encoding/json\""),
        ("import \"hex\"\nfn main() {\n    _ = hex.encode([]byte{1})\n}\n", "\"encoding/hex\""),
        ("import \"atomic\"\nfn main() {\n    var x atomic.int64\n    _ = x\n}\n", "\"sync/atomic\""),
        ("import \"filepath\"\nfn main() {\n    _ = filepath.base(\"a/b\")\n}\n", "\"path/filepath\""),
        // Full path imports remain preserved
        ("import \"net/http\"\nfn main() {\n    _ = http.status_ok\n}\n", "\"net/http\""),
        ("import \"encoding/json\"\nfn main() {\n    _ = json.marshal(1)\n}\n", "\"encoding/json\""),
        ("import \"sync/atomic\"\nfn main() {\n    var x atomic.int64\n    _ = x\n}\n", "\"sync/atomic\""),
        // Single segment std imports remain preserved
        ("import \"fmt\"\nfn main() {\n    fmt.println(1)\n}\n", "\"fmt\""),
        ("import \"os\"\nfn main() {\n    _ = os.args\n}\n", "\"os\""),
        // Third-party external imports remain preserved
        ("import \"example.com/custom/pkg\"\nfn main() {\n    _ = pkg.value\n}\n", "\"example.com/custom/pkg\""),
    ];

    for (src, expected_path) in cases {
        let ast = parse_file("test.hy", src).expect("must parse");
        let go = emit_go(&ast).expect("must emit");
        assert!(
            go.contains(expected_path),
            "expected emitted Go to import {}, but got:\n{}",
            expected_path,
            go
        );
    }
}

#[test]
fn test_curated_import_go_build_compilation() {
    let std_cases = [
        ("import_http", "package main\nimport \"http\"\nfn main() {\n    _ = http.status_ok\n}\n"),
        ("import_url", "package main\nimport \"url\"\nfn main() {\n    _ = url.query_escape(\"x\")\n}\n"),
        ("import_json", "package main\nimport \"json\"\nfn main() {\n    _, _ = json.marshal(1)\n}\n"),
        ("import_hex", "package main\nimport \"hex\"\nfn main() {\n    _ = hex.encode([]byte{1})\n}\n"),
        ("import_atomic", "package main\nimport \"atomic\"\nfn main() {\n    var x atomic.int64\n    _ = x\n}\n"),
        ("import_filepath", "package main\nimport \"filepath\"\nfn main() {\n    _ = filepath.base(\"a/b\")\n}\n"),
        ("import_httptest", "package main\nimport \"httptest\"\nfn main() {\n    _ = httptest.new_recorder()\n}\n"),
    ];

    let tmp_dir = std::env::temp_dir();
    for (name, src) in std_cases {
        let ast = parse_file("test.hy", src).expect("must parse");
        let go = emit_go(&ast).expect("must emit");

        let file_path = tmp_dir.join(format!("test_curated_{}.go", name));
        std::fs::write(&file_path, go.as_bytes()).expect("write go file");

        let out = Command::new("go")
            .args(["build", "-o", "/dev/null"])
            .arg(&file_path)
            .output()
            .expect("execute go build");

        let _ = std::fs::remove_file(&file_path);

        assert!(
            out.status.success(),
            "go build failed on emitted Go for case {}:\n---\n{}\n---\nstderr:\n{}",
            name,
            go,
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
