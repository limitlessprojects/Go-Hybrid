//! Regression tests for #3789:
//! 1. `httptest.get` / `httptest.post` lower to `strings.TrimSuffix` and `http.NewRequest`
//!    in `hy-emit`. When `strings` or `http` is shadowed in scope, `hy-check` must reject
//!    with `HYB-E1002`.
//! 2. E11 `http.handler` `?` propagation lowers to `http.Error(<w>, ..., http.StatusBadRequest)`.
//!    When `http` or the response writer parameter `<w>` is shadowed inside the closure body
//!    at the `?` site, `hy-check` must reject with `HYB-E1002`.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects_e1002(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {:?}, got {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(cs.is_empty(), "must accept {:?}, got {:?}", src, cs);
    assert_eq!(r.verdict, Verdict::Accept, "verdict for {:?}", src);
}

#[test]
fn test_httptest_get_with_package_const_strings_rejects() {
    let src = "import \"net/http\"\nimport \"net/http/httptest\"\n\nconst strings = \"shadowed\"\n\nfn main() {\n    mux := http.new_serve_mux()\n    code, body := httptest.get(mux, \"/health\")?\n    _ = code\n    _ = body\n}\n";
    rejects_e1002(src);
}

#[test]
fn test_httptest_post_with_local_strings_rejects() {
    let src = "import \"net/http\"\nimport \"net/http/httptest\"\n\nfn main() {\n    mux := http.new_serve_mux()\n    strings := 42\n    _ = strings\n    code, body := httptest.post(mux, \"/health\")?\n    _ = code\n    _ = body\n}\n";
    rejects_e1002(src);
}

#[test]
fn test_httptest_get_with_local_http_rejects() {
    let src = "import \"net/http\"\nimport \"net/http/httptest\"\n\nfn main() {\n    mux := http.new_serve_mux()\n    http := 1\n    _ = http\n    code, body := httptest.get(mux, \"/health\")?\n    _ = code\n    _ = body\n}\n";
    rejects_e1002(src);
}

#[test]
fn test_e11_handler_try_with_local_http_shadow_rejects() {
    let src = "import \"errors\"\nimport \"net/http\"\n\nfn step() error {\n    return errors.new(\"boom\")\n}\n\nfn main() {\n    h := http.handler(fn(w http.response_writer, r *http.request) {\n        http := 1\n        _ = http\n        step()?\n    })\n    _ = h\n}\n";
    rejects_e1002(src);
}

#[test]
fn test_e11_handler_try_with_writer_param_shadow_rejects() {
    let src = "import \"errors\"\nimport \"net/http\"\n\nfn step() error {\n    return errors.new(\"boom\")\n}\n\nfn main() {\n    h := http.handler(fn(w http.response_writer, r *http.request) {\n        if true {\n            w := 1\n            _ = w\n            step()?\n        }\n    })\n    _ = h\n}\n";
    rejects_e1002(src);
}

#[test]
fn test_positive_controls_unshadowed_httptest_and_e11_accept() {
    let src = "import \"errors\"\nimport \"net/http\"\nimport \"net/http/httptest\"\n\nfn step() error {\n    return errors.new(\"ok\")\n}\n\nfn main() {\n    mux := http.new_serve_mux()\n    h := http.handler(fn(w http.response_writer, r *http.request) {\n        step()?\n    })\n    _ = h\n    code, body := httptest.get(mux, \"/health\")?\n    _ = code\n    _ = body\n}\n";
    accepts(src);
}