use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn http_header_write_try_hoists_as_zero_result_error_only_call() {
    let src = r#"
import "net/http"

fn dump_headers(h http.header, w http.response_writer) (err error) {
    h.write(w)?
    return nil
}

fn main() {
}
"#;
    let parsed = parse_file("repro_http_header_write_try.hy", src).expect("parse ok");
    let go_code = emit_go(&parsed).expect("emit ok");
    assert!(
        go_code.contains(":= h.Write(w)") && !go_code.contains(", hyErr"),
        "expected error-only hoist `hyErrN := h.Write(w)` without phantom value temp, got:\n{}",
        go_code
    );
}

#[test]
fn http_response_writer_write_try_hoists_value_and_error_pair() {
    let src = r#"
import "net/http"

fn reply(w http.response_writer) (err error) {
    n := w.write([]byte("ok"))?
    _ = n
    return nil
}

fn main() {
}
"#;
    let parsed = parse_file("control_response_writer_write.hy", src).expect("parse ok");
    let go_code = emit_go(&parsed).expect("emit ok");
    assert!(
        go_code.contains("w.Write([]byte(\"ok\"))"),
        "expected w.Write emission, got:\n{}",
        go_code
    );
}
