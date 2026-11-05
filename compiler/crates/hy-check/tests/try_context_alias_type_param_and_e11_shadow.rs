use hy_check::{Verdict, check_file};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let parsed = parse_file("test.hy", src);
    assert!(
        parsed.diagnostics.is_empty(),
        "parse failed: {:?}",
        parsed.diagnostics
    );
    check_file(&parsed.file)
}

#[test]
fn rejects_try_when_type_param_shadows_error_or_error_alias_3746() {
    let r = check(
        "package main
         type failure = error
         struct holder[t any] { v t }
         fn val() (int, error) {
             return 1, nil
         }
         fn bad[error any](z error) (int, error) {
             x := val()?
             return x, z
         }
         fn bad_alias[failure any](z failure) (int, failure) {
             x := val()?
             return x, z
         }
         fn (h holder[error]) bad_recv(z error) (int, error) {
             x := val()?
             return x, z
         }
         fn main() {}",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    let e2011: Vec<_> = r
        .diagnostics
        .iter()
        .filter(|d| d.code == "HYB-E2011")
        .collect();
    assert_eq!(e2011.len(), 3, "{:?}", r.diagnostics);
}

#[test]
fn rejects_try_in_e11_closure_when_writer_or_http_is_shadowed_3746() {
    let r = check(
        "package main
         import "net/http"
         fn close_it() error {
             return nil
         }
         fn main() {
             mux := http.new_serve_mux()
             mux.handle("/a", http.handler(fn(w, r) {
                 _ = w
                 _ = r
                 w := 1
                 _ = w
                 close_it()?
             }))
             mux.handle("/b", http.handler(fn(w, http) {
                 _ = w
                 _ = http
                 close_it()?
             }))
             mux.handle("/c", http.handler(fn(w, r) {
                 _ = w
                 _ = r
                 http := 1
                 _ = http
                 close_it()?
             }))
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    let e2011: Vec<_> = r
        .diagnostics
        .iter()
        .filter(|d| d.code == "HYB-E2011")
        .collect();
    assert_eq!(e2011.len(), 3, "{:?}", r.diagnostics);
}

#[test]
fn accepts_valid_generic_error_result_and_unshadowed_e11_closure_3746() {
    let r = check(
        "package main
         import "net/http"
         type failure = error
         fn val() (int, error) {
             return 1, nil
         }
         fn close_it() error {
             return nil
         }
         fn ok_gen[t any](v t) (t, failure) {
             _ = val()?
             return v, nil
         }
         fn main() {
             mux := http.new_serve_mux()
             mux.handle("/ok", http.handler(fn(w, r) {
                 _ = w
                 _ = r
                 close_it()?
             }))
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}
