//! place_receiver_shapes_issue_2128.rs
//! Regression test for issue #2128:
//! hy-emit: assignment target Place receiver misses foreign MethodZero ()
//! and nested struct foreign shapes.

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit_src(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("parses");
    emit_go(&ast).expect("emits")
}

#[test]
fn test_place_method_zero_accessor_with_index() {
    let src = r#"
package main

import "net/http"

fn update_header(w http.response_writer) {
    w.header["X-Mode"] = []string{"fast"}
}
"#;
    let go = emit_src(src);
    assert!(
        go.contains(r#"w.Header()["X-Mode"] = []string{"fast"}"#),
        "expected w.Header()() call on LHS of assignment, got:\n{go}"
    );
}

#[test]
fn test_place_foreign_field_nested_in_user_struct() {
    let src = r#"
package main

import "net/http"

struct req_ctx {
    req *http.request
}

struct outer_ctx {
    inner req_ctx
}

fn update_req(c *req_ctx, o *outer_ctx) {
    c.req.host = "example.com"
    o.inner.req.host = "nested.example.com"
}
"#;
    let go = emit_src(src);
    assert!(
        go.contains(r#"c.req.Host = "example.com""#),
        "expected c.req.Host uppercase Go field, got:\n{go}"
    );
    assert!(
        go.contains(r#"o.inner.req.Host = "nested.example.com""#),
        "expected o.inner.req.Host uppercase Go field, got:\n{go}"
    );
}
