//! Regression tests for E4 / E11 `?` propagation context enforcement (`HYB-E2011`)
//! across `http.handler`-returning functions, malformed/shadowed `http.handler`
//! closures, file-level and local `const` initializers, type expressions, and
//! shadowed `error` result types.

use hy_check::{check_file, CheckResult};
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
fn return_fnlit_in_http_handler_fn_allows_e11_try() {
    for (label, src) in [
        (
            "untyped params in http.handler-returning fn",
            "import \"net/http\"\n\nfn make_handler() http.handler {\n    return fn(w, r) {\n        x := val()?\n        fmt.fprintf(w, \"{x}\\n\")\n    }\n}\n\nfn main() {\n    _ = make_handler()\n}\n",
        ),
        (
            "explicitly typed params in http.handler-returning fn",
            "import \"net/http\"\n\nfn make_handler() http.handler {\n    return fn(w http.response_writer, r *http.request) {\n        x := val()?\n        fmt.fprintf(w, \"{x}\\n\")\n    }\n}\n\nfn main() {\n    _ = make_handler()\n}\n",
        ),
    ] {
        let r = with_helpers(src);
        assert_eq!(e2011(&r), 0, "{label} must accept E11 `?`: {:?}", r.diagnostics);
    }
}

#[test]
fn malformed_e11_http_handler_closures_and_calls_reject_e2011() {
    for (label, src) in [
        (
            "blank writer param in http.handler call",
            "import \"net/http\"\n\nfn main() {\n    mux := http.new_serve_mux()\n    mux.handle(\"/x\", http.handler(fn(_, r) {\n        close_it()?\n    }))\n}\n",
        ),
        (
            "blank writer param in http.handler return",
            "import \"net/http\"\n\nfn make_h() http.handler {\n    return fn(_, r) {\n        close_it()?\n    }\n}\n\nfn main() {\n    _ = make_h()\n}\n",
        ),
        (
            "wrong param arity (1 param) in http.handler call",
            "import \"net/http\"\n\nfn main() {\n    mux := http.new_serve_mux()\n    mux.handle(\"/x\", http.handler(fn(w) {\n        close_it()?\n    }))\n}\n",
        ),
        (
            "non-empty results on http.handler closure",
            "import \"net/http\"\n\nfn main() {\n    mux := http.new_serve_mux()\n    mux.handle(\"/x\", http.handler(fn(w, r) int {\n        x := val()?\n        return x\n    }))\n}\n",
        ),
        (
            "parenthesized callee (http.handler)(fn(w, r) { ... })",
            "import \"net/http\"\n\nfn main() {\n    mux := http.new_serve_mux()\n    mux.handle(\"/x\", (http.handler)(fn(w, r) {\n        close_it()?\n    }))\n}\n",
        ),
        (
            "parenthesized closure arg http.handler((fn(w, r) { ... }))",
            "import \"net/http\"\n\nfn main() {\n    mux := http.new_serve_mux()\n    mux.handle(\"/x\", http.handler((fn(w, r) {\n        close_it()?\n    })))\n}\n",
        ),
        (
            "local binding shadowing http package root",
            "struct fake_http {}\n\nfn (h fake_http) handler(f fn(int, int)) int {\n    return 0\n}\n\nfn main() {\n    http := fake_http{}\n    _ = http.handler(fn(w int, r int) {\n        close_it()?\n    })\n}\n",
        ),
    ] {
        let r = with_helpers(src);
        assert_eq!(e2011(&r), 1, "{label} must reject with HYB-E2011: {:?}", r.diagnostics);
    }
}

#[test]
fn const_initializers_and_shadowed_error_results_reject_e2011() {
    for (label, src) in [
        (
            "file-level composite const initializer with `?`",
            "const xs = []int{val()?}\n\nfn main() {\n    fmt.println(len(xs))\n}\n",
        ),
        (
            "closure inside file-level const initializer with `?`",
            "const fs = []fn(){fn() { close_it()? }}\n\nfn main() {\n    fmt.println(len(fs))\n}\n",
        ),
        (
            "local composite const initializer with `?` inside main",
            "fn main() {\n    const xs = []int{val()?}\n    fmt.println(len(xs))\n}\n",
        ),
        (
            "user struct shadowing predeclared error in fn result",
            "struct error {}\n\nfn bad() (int, error) {\n    x := val()?\n    return x, error{}\n}\n\nfn main() {\n    _, _ = bad()\n}\n",
        ),
    ] {
        let r = with_helpers(src);
        assert_eq!(e2011(&r), 1, "{label} must reject with HYB-E2011: {:?}", r.diagnostics);
    }
}
