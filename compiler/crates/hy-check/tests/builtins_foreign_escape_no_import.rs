//! Regression tests for issue #4493: the A5 foreign-escape check-site
//! (`HYB-E3013`) must fire for builtin foreign packages (`http`, ...) even
//! when the file omits the `import` line. Name resolution and the emitter
//! both recognize those roots without an import, so the escape check must
//! not depend on the file's import declarations either.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn assert_rejects_code(src: &str, want: &str) {
    let r = check_file(&parse_file("test.hy", src).expect("parse"));
    assert!(r.verdict == Verdict::Reject && r.diagnostics.iter().any(|d| d.code == want), "{:?}", r.diagnostics);
}

fn assert_clean(src: &str) {
    let r = check_file(&parse_file("test.hy", src).expect("parse"));
    assert_eq!(r.verdict, Verdict::Accept, "unexpected {:?}: {:?} for:\n{src}", r.verdict, r.diagnostics);
}

#[test]
fn corpus10_shape_without_import_rejects_e3013() {
    // Checker corpus 10 verbatim except for its `import "net/http"` line.
    assert_rejects_code(
        "fn main() {\nhits := 0\nmux := http.new_serve_mux()\nmux.handle(\"/ping\", http.handler(fn(w http.response_writer, r *http.request) {\nhits++\nfmt.fprintln(w, \"pong {hits}\")\n}))\nfor _ in 0..3 {\ncode, body := httptest.get(mux, \"/ping\")?\nfmt.println(\"{code} {body}\")\n}\n}",
        "HYB-E3013",
    );
}

#[test]
fn direct_and_bound_forms_without_import_reject_e3013() {
    for src in [
        // Direct package call, literal closure.
        "fn main() {\nhits := 0\nhttp.handle_func(\"/a\", fn(w http.response_writer, r *http.request) { hits++ })\n}",
        // Bound closure variable passed to a package call.
        "fn main() {\nhits := 0\ncb := fn(w http.response_writer, r *http.request) { hits++ }\nhttp.handle_func(\"/a\", cb)\n}",
        // Registration from inside a nested closure.
        "fn main() {\nhits := 0\nregister := fn() {\nhttp.handle_func(\"/a\", fn(w http.response_writer, r *http.request) { hits++ })\n}\nregister()\n}",
        // Another builtin root: time.after_func.
        "fn main() {\nhits := 0\n_ = time.after_func(0, fn() { hits++ })\n_ = hits\n}",
    ] {
        assert_rejects_code(src, "HYB-E3013");
    }
}

#[test]
fn no_import_controls_stay_accepted() {
    // Read-only handler: no captured write, no escape.
    assert_clean("fn main() {\nhits := 0\nhttp.handle_func(\"/a\", fn(w http.response_writer, r *http.request) { fmt.fprintln(w, \"pong {hits}\") })\n_ = hits\n}");
    // The declared hatch works without an import too.
    assert_clean("fn main() {\nhits := 0\nmanaged(\"counter\", \"interop\", \"#1\") { http.handle_func(\"/a\", fn(w http.response_writer, r *http.request) { hits++ }) }\n_ = hits\n}");
    // A writing closure passed to a LOCAL function is not a foreign escape.
    assert_clean("fn run(f fn()) { f() }\nfn main() {\nhits := 0\nrun(fn() { hits++ })\n_ = hits\n}");
}
