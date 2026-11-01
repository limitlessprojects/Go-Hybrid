use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let ast = parse_file("call_and_try_receiver_shapes.hy", src).expect("parse");
    emit_go(&ast).expect("emit_go")
}

#[test]
fn call_and_try_receivers_resolve_foreign_member_shapes() {
    let go = emit(
        r#"
import "net/http"

fn pick_req(r *http.request) *http.request {
    return r
}

fn load_req(r *http.request) (*http.request, error) {
    return r, nil
}

fn probe(r *http.request) (string, error) {
    t0 := time.now()
    ms := time.since(t0).milliseconds
    sec := time.seconds(2).milliseconds
    host := pick_req(r).url.host
    mode := load_req(r)?.url.query.get("mode")
    if ms >= 0 && sec == 2000 && host != "" {
        return mode, nil
    }
    return "", nil
}

fn main() {
}
"#,
    );
    assert!(
        go.contains("time.Since(t0).Milliseconds()"),
        "expected time.Since(t0) receiver to resolve MethodZero Milliseconds(), got:\n{go}"
    );
    assert!(
        go.contains("(time.Duration(2) * time.Second).Milliseconds()"),
        "expected time.seconds(2) receiver to resolve MethodZero Milliseconds(), got:\n{go}"
    );
    assert!(
        go.contains("pick_req(r).URL.Host"),
        "expected user fn call returning *http.request to resolve .URL.Host, got:\n{go}"
    );
    assert!(
        go.contains(".URL.Query().Get(\"mode\")"),
        "expected ?-unwrapped call receiver to resolve .URL.Query().Get, got:\n{go}"
    );
}
