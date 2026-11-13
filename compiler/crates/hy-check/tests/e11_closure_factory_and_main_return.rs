//! Regression tests for E11 `return_e11` alignment between `hy-check` and `hy-emit`:
//! closure factories `fn() http.handler { return fn(w, r) { ... ? } }` and
//! rejecting unlowered `return fn(w, r) { ... ? }` inside `fn main() http.handler`.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn e2011(r: &CheckResult) -> usize {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error && d.code == "HYB-E2011")
        .count()
}

#[test]
fn closure_factory_returning_http_handler_accepts_e11_try() {
    let r = check(
        "import \"net/http\"\nimport \"strconv\"\n\nfn main() {\n    top := strconv.atoi(\"1\")?\n    _ = top\n    mk := fn() http.handler {\n        return fn(w, r) {\n            n := strconv.atoi(\"42\")?\n            _ = n\n            _ = w\n            _ = r\n        }\n    }\n    nested := fn() fn() http.handler {\n        return fn() http.handler {\n            return fn(w, r) {\n                m := strconv.atoi(\"7\")?\n                _ = m\n                _ = w\n                _ = r\n            }\n        }\n    }\n    _ = mk\n    _ = nested\n}\n",
    );
    assert_eq!(e2011(&r), 0, "unexpected HYB-E2011: {:?}", r.diagnostics);
    assert_eq!(r.verdict, Verdict::Accept, "diags: {:?}", r.diagnostics);
}

#[test]
fn closure_factory_does_not_leak_return_e11_into_inner_non_handler_closure() {
    let r = check(
        "import \"net/http\"\nimport \"strconv\"\n\nfn main() {\n    mk := fn() http.handler {\n        bad := fn() {\n            n := strconv.atoi(\"1\")?\n            _ = n\n        }\n        _ = bad\n        return fn(w, r) {\n            _ = w\n            _ = r\n        }\n    }\n    _ = mk\n}\n",
    );
    assert_eq!(e2011(&r), 1, "inner result-less closure must still reject HYB-E2011: {:?}", r.diagnostics);
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn closure_factory_shadowed_http_or_named_result_rejects_e2011() {
    for (label, src) in [
        (
            "closure param shadows http root",
            "import \"net/http\"\nimport \"strconv\"\n\nfn main() {\n    mk := fn(http int) http.handler {\n        _ = http\n        return fn(w, r) {\n            n := strconv.atoi(\"1\")?\n            _ = n\n            _ = w\n            _ = r\n        }\n    }\n    _ = mk\n}\n",
        ),
        (
            "local binding before return shadows http root",
            "import \"net/http\"\nimport \"strconv\"\n\nfn main() {\n    mk := fn() http.handler {\n        http := 1\n        _ = http\n        return fn(w, r) {\n            n := strconv.atoi(\"1\")?\n            _ = n\n            _ = w\n            _ = r\n        }\n    }\n    _ = mk\n}\n",
        ),
        (
            "named result on closure factory is not lowered as E11",
            "import \"net/http\"\nimport \"strconv\"\n\nfn main() {\n    mk := fn() (h http.handler) {\n        return fn(w, r) {\n            n := strconv.atoi(\"1\")?\n            _ = n\n            _ = w\n            _ = r\n        }\n    }\n    _ = mk\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(e2011(&r), 1, "{label} must reject with HYB-E2011: {:?}", r.diagnostics);
        assert_eq!(r.diagnostics.len(), 1, "{label} must emit only HYB-E2011: {:?}", r.diagnostics);
        assert_eq!(r.verdict, Verdict::Reject);
    }
}

#[test]
fn main_declaring_http_handler_result_rejects_unlowered_e11_return() {
    let r = check(
        "import \"net/http\"\nimport \"strconv\"\n\nfn main() http.handler {\n    return fn(w, r) {\n        n := strconv.atoi(\"1\")?\n        _ = n\n        _ = w\n        _ = r\n    }\n}\n",
    );
    assert_eq!(e2011(&r), 1, "main() http.handler must reject E11 return: {:?}", r.diagnostics);
    assert_eq!(r.diagnostics.len(), 1, "expected only HYB-E2011: {:?}", r.diagnostics);
    assert_eq!(r.verdict, Verdict::Reject);
}
