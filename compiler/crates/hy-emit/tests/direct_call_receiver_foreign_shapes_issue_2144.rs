use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("parse");
    emit_go(&ast).expect("emit_go")
}

#[test]
fn test_direct_call_and_try_receiver_shapes_issue_2144() {
    let src = r#"
package main

import "fmt"
import "net/http"
import "time"

fn pick_req(r *http.request) *http.request {
    return r
}

fn load_req(r *http.request) (*http.request, error) {
    return r, nil
}

fn probe(r *http.request, t0 time.time) (string, error) {
    ms := time.since(t0).milliseconds
    h1 := pick_req(r).url.host
    h2 := load_req(r)?.url.host
    return fmt.sprintf("%d:%s:%s", ms, h1, h2), nil
}

fn main() {}
"#;

    let go = emit(src);
    assert!(
        go.contains("time.Since(t0).Milliseconds()"),
        "expected time.Since(t0).Milliseconds(), got:\n{}",
        go
    );
    assert!(
        go.contains("pick_req(r).URL.Host"),
        "expected pick_req(r).URL.Host, got:\n{}",
        go
    );
    assert!(
        go.contains(".URL.Host"),
        "expected hoisted try temp to access .URL.Host, got:\n{}",
        go
    );
}

#[test]
fn test_package_constructor_duration_and_serde_direct_call_receivers() {
    let src = r#"
package main

import "time"
import "rust/serde_json"

fn test_duration() int64 {
    return time.seconds(2).milliseconds
}

fn test_serde(raw string) (int64, error) {
    return serde_json.parse(raw)?.int("count")?
}

fn main() {}
"#;

    let go = emit(src);
    assert!(
        go.contains(".Milliseconds()"),
        "expected .Milliseconds(), got:\n{}",
        go
    );
    assert!(
        go.contains(".Int(\"count\")"),
        "expected .Int(\"count\"), got:\n{}",
        go
    );
}
