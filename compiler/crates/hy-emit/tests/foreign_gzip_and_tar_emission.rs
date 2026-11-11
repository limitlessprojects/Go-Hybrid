use hy_emit::emit_go;
use hy_syntax::parse_file;
use std::process::Command;

#[test]
fn test_gzip_and_tar_method_and_header_surfaces_emit_and_build() {
    let src = r#"package main
import "gzip"
import "os"
import "tar"

fn exercise_gzip(gh *gzip.header) error {
    gw := gzip.new_writer(os.stdout)
    gh.name = "payload.txt"
    gh.comment = "compressed"
    gh.os = 255
    _ = gh.extra
    _ = gh.mod_time
    _, _ = gw.write([]byte("hello"))
    _ = gw.flush()
    gw.reset(os.stdout)
    _ = gw.close()

    gr := gzip.new_reader(os.stdin)?
    gr.multistream(false)
    out := make([]byte, 8)
    _, _ = gr.read(out)
    _ = gr.reset(os.stdin)
    return gr.close()
}

fn exercise_tar(hdr *tar.header) error {
    hdr.name = "entry.txt"
    hdr.linkname = ""
    hdr.size = 5
    hdr.mode = 420
    hdr.typeflag = tar.type_reg
    hdr.uname = "root"
    hdr.gname = "root"
    _ = hdr.mod_time
    _ = hdr.access_time
    _ = hdr.change_time
    _ = hdr.devmajor
    _ = hdr.devminor
    _ = hdr.pax_records
    _ = hdr.format
    _ = hdr.file_info()

    tw := tar.new_writer(os.stdout)
    _ = tw.write_header(hdr)
    _, _ = tw.write([]byte("hello"))
    _ = tw.flush()
    if false {
        _ = tw.add_fs(os.dir_fs("."))
    }
    _ = tw.close()

    tr := tar.new_reader(os.stdin)
    _, _ = tr.next()
    out := make([]byte, 5)
    _, _ = tr.read(out)
    return nil
}

fn main() {
    var gh gzip.header
    var hdr tar.header
    _ = exercise_gzip(&gh)
    _ = exercise_tar(&hdr)
}
"#;

    let ast = parse_file("test.hy", src).expect("must parse");
    let go = emit_go(&ast).expect("must emit");

    for expected in [
        "gzip.NewWriter(os.Stdout)",
        "gh.Name = \"payload.txt\"",
        "gh.Comment = \"compressed\"",
        "gh.OS = 255",
        "gh.Extra",
        "gh.ModTime",
        "gw.Write([]byte(\"hello\"))",
        "gw.Flush()",
        "gw.Reset(os.Stdout)",
        "gw.Close()",
        "gzip.NewReader(os.Stdin)",
        "gr.Multistream(false)",
        "gr.Read(out)",
        "gr.Reset(os.Stdin)",
        "gr.Close()",
        "hdr.Name = \"entry.txt\"",
        "hdr.Size = 5",
        "hdr.Mode = 420",
        "hdr.Typeflag = tar.TypeReg",
        "hdr.PAXRecords",
        "hdr.FileInfo()",
        "tar.NewWriter(os.Stdout)",
        "tw.WriteHeader(hdr)",
        "tw.Write([]byte(\"hello\"))",
        "tw.Flush()",
        "tw.AddFS(os.DirFS(\".\"))",
        "tw.Close()",
        "tar.NewReader(os.Stdin)",
        "tr.Next()",
        "tr.Read(out)",
    ] {
        assert!(
            go.contains(expected),
            "expected emitted Go to contain `{expected}`, got:\n{go}"
        );
    }

    let tmp_dir = std::env::temp_dir();
    let file_path = tmp_dir.join("test_foreign_gzip_and_tar_emission.go");
    std::fs::write(&file_path, go.as_bytes()).expect("write go file");

    let out = Command::new("go")
        .args(["build", "-o", "/dev/null"])
        .arg(&file_path)
        .output()
        .expect("execute go build");

    let _ = std::fs::remove_file(&file_path);

    assert!(
        out.status.success(),
        "go build failed on emitted Go:\n---\n{go}\n---\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
