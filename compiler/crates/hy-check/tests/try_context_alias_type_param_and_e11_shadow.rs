//! Soundness (GUARANTEES Row 13 — BUILDABLE & E4/E11 `HYB-E2011`):
//!   1. Generic type parameters named `error` (`fn f[error any](z error) (int, error)`),
//!      generic type parameters shadowing a type alias to `error`
//!      (`type failure = error; fn f[failure any](z failure) (int, failure)`),
//!      or receiver type parameters (`struct box[error any] {}; fn (b box[error]) f(z error) (int, error)`)
//!      shadow the predeclared `error` interface in the function's results and must
//!      reject `?` with `HYB-E2011`.
//!   2. Inside an E11 `http.handler` closure (`http.handler(fn(w, r) { ... })` or
//!      `return fn(w, r) { ... }` in an `http.handler`-returning fn/closure),
//!      `hy-emit` lowers `?` to `http.Error(<w>, <err>.Error(), http.StatusBadRequest)`.
//!      If `http` or the writer parameter `<w>` is shadowed by a closure parameter
//!      (`fn(http, r)`, `fn(w, http)`, `fn(w, w)`) or by an enclosing local/loop
//!      binding at the `?` site, `HYB-E2011` must be emitted.
//!   3. Array-length expressions (`[N]T`) inside an E11 `http.handler` closure must
//!      also reject `?` with `HYB-E2011`.

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

const HELPERS: &str =
    "fn val() (int, error) {\n    return 1, nil\n}\n\nfn close_it() error {\n    return nil\n}\n\n";

fn with_helpers(body: &str) -> CheckResult {
    check(&format!("{HELPERS}{body}"))
}

#[test]
fn generic_type_param_and_receiver_param_shadowing_error_reject_e2011() {
    for (label, src) in [
        (
            "fn generic type param named `error`",
            "fn bad[error any](z error) (int, error) {\n    x := val()?\n    return x, z\n}\n\nfn main() {\n    _, _ = bad(0)\n}\n",
        ),
        (
            "fn generic type param shadowing type alias `type failure = error`",
            "type failure = error\n\nfn bad[failure any](z failure) (int, failure) {\n    x := val()?\n    return x, z\n}\n\nfn main() {\n    _, _ = bad(0)\n}\n",
        ),
        (
            "method receiver type param named `error`",
            "struct holder[t any] { v t }\n\nfn (h holder[error]) bad(z error) (int, error) {\n    x := val()?\n    return x, z\n}\n\nfn main() {\n    h := holder[int]{v: 1}\n    _, _ = h.bad(0)\n}\n",
        ),
        (
            "fn generic type param named `http` on http.handler factory",
            "import \"net/http\"\n\nfn make_h[http any](x http) http.handler {\n    _ = x\n    return fn(w, r) {\n        _ = w\n        _ = r\n        close_it()?\n    }\n}\n\nfn main() {}\n",
        ),
    ] {
        let r = with_helpers(src);
        assert_eq!(e2011(&r), 1, "{label} must reject with HYB-E2011: {:?}", r.diagnostics);
        assert_eq!(r.verdict, Verdict::Reject);
    }
}

#[test]
fn e11_handler_closure_shadowing_writer_or_http_at_try_site_rejects_e2011() {
    for (label, src) in [
        (
            "writer param named `http` in http.handler(fn(http, r))",
            "import \"net/http\"\n\nfn main() {\n    mux := http.new_serve_mux()\n    mux.handle(\"/x\", http.handler(fn(http, r) {\n        _ = http\n        _ = r\n        close_it()?\n    }))\n}\n",
        ),
        (
            "request param named `http` in http.handler(fn(w, http))",
            "import \"net/http\"\n\nfn main() {\n    mux := http.new_serve_mux()\n    mux.handle(\"/x\", http.handler(fn(w, http) {\n        _ = w\n        _ = http\n        close_it()?\n    }))\n}\n",
        ),
        (
            "local `w := 1` shadowing writer param before `?`",
            "import \"net/http\"\n\nfn main() {\n    mux := http.new_serve_mux()\n    mux.handle(\"/x\", http.handler(fn(w, r) {\n        _ = w\n        _ = r\n        w := 1\n        _ = w\n        close_it()?\n    }))\n}\n",
        ),
        (
            "local `http := 1` shadowing http package inside E11 closure before `?`",
            "import \"net/http\"\n\nfn main() {\n    mux := http.new_serve_mux()\n    mux.handle(\"/x\", http.handler(fn(w, r) {\n        _ = w\n        _ = r\n        http := 1\n        _ = http\n        close_it()?\n    }))\n}\n",
        ),
        (
            "for-range var shadowing writer param `w` before `?` in http.handler-returning fn",
            "import \"net/http\"\n\nfn make_h() http.handler {\n    return fn(w, r) {\n        _ = w\n        _ = r\n        for w in 0..1 {\n            _ = w\n            close_it()?\n        }\n    }\n}\n\nfn main() {\n    _ = make_h()\n}\n",
        ),
        (
            "array length `[N]T` with `?` inside E11 http.handler closure",
            "import \"net/http\"\n\nfn main() {\n    mux := http.new_serve_mux()\n    mux.handle(\"/x\", http.handler(fn(w, r) {\n        var a [val()?]int\n        _ = a\n        _ = w\n        _ = r\n    }))\n}\n",
        ),
    ] {
        let r = with_helpers(src);
        assert_eq!(e2011(&r), 1, "{label} must reject with HYB-E2011: {:?}", r.diagnostics);
        assert_eq!(r.verdict, Verdict::Reject);
    }
}

#[test]
fn valid_error_results_and_unshadowed_e11_handlers_stay_accepted() {
    let r = with_helpers(
        "import \"net/http\"\n\ntype failure = error\n\nfn ok_fn() (int, failure) {\n    x := val()?\n    return x, nil\n}\n\nfn make_h() http.handler {\n    return fn(w, r) {\n        x := val()?\n        if true {\n            w := 1\n            _ = w\n        }\n        y := val()?\n        fmt.fprintf(w, \"{x} {y}\\n\")\n        _ = r\n    }\n}\n\nfn main() {\n    _, _ = ok_fn()?\n    _ = make_h()\n}\n",
    );
    assert_eq!(e2011(&r), 0, "unexpected HYB-E2011: {:?}", r.diagnostics);
    assert_eq!(r.verdict, Verdict::Accept, "diags: {:?}", r.diagnostics);
}
