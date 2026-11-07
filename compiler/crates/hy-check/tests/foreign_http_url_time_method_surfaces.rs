//! Regression tests for `foreign.rs` (`foreign_method_surface` and `method_result`)
//! gating phantom methods on `http.handler`, `http.response_writer`, `http.serve_mux`,
//! `http.header` (including `w.header()`), `url.values`, and `time.duration`
//! (`HYB-E1002`, GUARANTEES Row 13 `BUILDABLE`).

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let file = parse_file("test.hy", src).expect("parses");
    check_file(&file)
        .diagnostics
        .into_iter()
        .map(|d| d.code.to_string())
        .collect()
}

#[test]
fn rejects_phantom_methods_on_http_url_and_time_duration_receivers() {
    assert_eq!(
        codes(
            r#"import "fmt"
import "net/http"
fn main() {
    mux := http.new_serve_mux()
    mux.totally_fake()
    fmt.println(1)
}
"#,
        ),
        vec!["HYB-E1002"],
        "phantom method on `http.serve_mux` must reject with HYB-E1002"
    );

    assert_eq!(
        codes(
            r#"import "fmt"
import "net/http"
fn run(h http.handler, w http.response_writer) {
    h.totally_fake()
    w.totally_fake()
    hdr := w.header()
    hdr.totally_fake()
    fmt.println(1)
}
fn main() {
    fmt.println(1)
}
"#,
        ),
        vec!["HYB-E1002", "HYB-E1002", "HYB-E1002"],
        "phantom methods on `http.handler`, `http.response_writer`, and `w.header()` must reject with HYB-E1002"
    );

    assert_eq!(
        codes(
            r#"import "fmt"
import "net/url"
import "time"
fn run(v url.values, d time.duration) {
    v.totally_fake()
    d.totally_fake()
    fmt.println(1)
}
fn main() {
    fmt.println(1)
}
"#,
        ),
        vec!["HYB-E1002", "HYB-E1002"],
        "phantom methods on `url.values` and `time.duration` must reject with HYB-E1002"
    );
}

#[test]
fn accepts_valid_methods_on_http_url_and_time_duration_receivers() {
    assert!(
        codes(
            r#"import "fmt"
import "net/http"
import "net/url"
import "time"
fn run(h http.handler, w http.response_writer, r *http.request, v url.values, d time.duration) {
    w.header().set("Content-Type", "text/plain")
    w.write_header(200)
    h.serve_http(w, r)
    v.set("k", "val")
    fmt.println(v.get("k"), d.milliseconds())
}
fn main() {
    mux := http.new_serve_mux()
    _ = mux
    fmt.println(1)
}
"#,
        )
        .is_empty(),
        "real methods on `http.handler`, `http.response_writer`, `http.header`, `url.values`, and `time.duration` must pass with 0 diagnostics"
    );
}
