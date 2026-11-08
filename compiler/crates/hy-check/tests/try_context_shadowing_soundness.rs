//! Regression tests for E4 / E6 / E11 `?` propagation context enforcement (`HYB-E2011`)
//! when `http`, the E11 `http.handler` writer parameter, predeclared `error` (or an
//! `error` type alias), or `main`'s E6 wrapper error variable `hyErr` is shadowed by
//! closure parameters, inner lexical bindings, generic type parameters, or file-level
//! value declarations.

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

const HELPERS: &str = r#"fn val() (int, error) {
    return 1, nil
}

fn close_it() error {
    return nil
}

"#;

fn with_helpers(body: &str) -> CheckResult {
    check(&format!("{HELPERS}{body}"))
}

#[test]
fn e11_handler_writer_or_http_shadowing_rejects_e2011() {
    for (label, src) in [
        (
            "http.handler closure first param named http",
            r#"import "net/http"

fn main() {
    mux := http.new_serve_mux()
    mux.handle("/x", http.handler(fn(http, r) {
        close_it()?
        _ = http
        _ = r
    }))
}
"#,
        ),
        (
            "http.handler closure second param named http",
            r#"import "net/http"

fn main() {
    mux := http.new_serve_mux()
    mux.handle("/x", http.handler(fn(w, http) {
        close_it()?
        _ = w
        _ = http
    }))
}
"#,
        ),
        (
            "inner block local shadowing writer param before try",
            r#"import "net/http"

fn main() {
    mux := http.new_serve_mux()
    mux.handle("/x", http.handler(fn(w, r) {
        if true {
            w := 1
            _ = w
            close_it()?
        }
        _ = w
        _ = r
    }))
}
"#,
        ),
        (
            "inner block local shadowing http package root before try",
            r#"import "net/http"

fn main() {
    mux := http.new_serve_mux()
    mux.handle("/x", http.handler(fn(w, r) {
        if true {
            http := 1
            _ = http
            close_it()?
        }
        _ = w
        _ = r
    }))
}
"#,
        ),
        (
            "for-range variable shadowing writer param before try",
            r#"import "net/http"

fn main() {
    mux := http.new_serve_mux()
    mux.handle("/x", http.handler(fn(w, r) {
        for w in 0..1 {
            _ = w
            close_it()?
        }
        _ = w
        _ = r
    }))
}
"#,
        ),
        (
            "try-define binding http directly inside http.handler closure",
            r#"import "net/http"

fn main() {
    mux := http.new_serve_mux()
    mux.handle("/x", http.handler(fn(w, r) {
        http := val()?
        _ = http
        _ = w
        _ = r
    }))
}
"#,
        ),
        (
            "try-define binding writer param in inner block of http.handler closure",
            r#"import "net/http"

fn main() {
    mux := http.new_serve_mux()
    mux.handle("/x", http.handler(fn(w, r) {
        if true {
            w := val()?
            _ = w
        }
        _ = w
        _ = r
    }))
}
"#,
        ),
    ] {
        let r = with_helpers(src);
        assert_eq!(
            e2011(&r),
            1,
            "{label} must reject with HYB-E2011: {:?}",
            r.diagnostics
        );
        assert_eq!(
            r.diagnostics.len(),
            1,
            "{label} must emit only HYB-E2011: {:?}",
            r.diagnostics
        );
        assert_eq!(r.verdict, Verdict::Reject);
    }
}

#[test]
fn generic_type_params_shadowing_error_or_http_reject_e2011() {
    for (label, src) in [
        (
            "generic fn type param [error] shadowing error result",
            r#"fn bad[error](x error) error {
    close_it()?
    return x
}

fn main() {
    _ = bad(0)
}
"#,
        ),
        (
            "generic struct receiver type param [error] shadowing error result",
            r#"struct holder[t] {
    v t
}

fn (h holder[error]) bad() error {
    close_it()?
    return h.v
}

fn main() {
    b := holder[int]{v: 1}
    _ = b.bad()
}
"#,
        ),
        (
            "generic fn type param [failure] shadowing error type alias result",
            r#"type failure = error

fn bad[failure](x failure) failure {
    close_it()?
    return x
}

fn main() {
    _ = bad(0)
}
"#,
        ),
        (
            "generic fn type param [http] shadowing http.handler return contract",
            r#"import "net/http"

fn make_h[http](x http) http.handler {
    _ = x
    return fn(w, r) {
        close_it()?
        _ = w
        _ = r
    }
}

fn main() {
    mux := http.new_serve_mux()
    _ = mux
    _ = make_h(0)
}
"#,
        ),
        (
            "generic method receiver type param [http] shadowing http.handler return",
            r#"import "net/http"

struct holder[t] {
    v t
}

fn (h holder[http]) make_h() http.handler {
    _ = h.v
    return fn(w, r) {
        close_it()?
        _ = w
        _ = r
    }
}

fn main() {
    mux := http.new_serve_mux()
    _ = mux
    b := holder[int]{v: 1}
    _ = b.make_h()
}
"#,
        ),
    ] {
        let r = with_helpers(src);
        assert_eq!(
            e2011(&r),
            1,
            "{label} must reject with HYB-E2011: {:?}",
            r.diagnostics
        );
        assert_eq!(
            r.diagnostics.len(),
            1,
            "{label} must emit only HYB-E2011: {:?}",
            r.diagnostics
        );
        assert_eq!(r.verdict, Verdict::Reject);
    }
}

#[test]
fn value_and_lexical_shadowing_of_error_rejects_e2011() {
    for (label, src) in [
        (
            "file-level const named error shadowing fn error result",
            r#"const error = 1

fn bad() (int, error) {
    x := val()?
    return x, nil
}

fn main() {
    _ = error
    _, _ = bad()
}
"#,
        ),
        (
            "file-level fn named error shadowing fn error result",
            r#"fn error() int {
    return 1
}

fn bad() (int, error) {
    x := val()?
    return x, nil
}

fn main() {
    _ = error()
    _, _ = bad()
}
"#,
        ),
        (
            "enclosing param named error shadowing closure error result",
            r#"fn outer(error int) {
    _ = error
    f := fn() (int, error) {
        x := val()?
        return x, nil
    }
    _, _ = f()
}

fn main() {
    outer(1)
}
"#,
        ),
        (
            "method receiver named error shadowing closure error result",
            r#"struct box {
    v int
}

fn (error box) work() {
    _ = error.v
    f := fn() (int, error) {
        x := val()?
        return x, nil
    }
    _, _ = f()
}

fn main() {
    b := box{v: 1}
    b.work()
}
"#,
        ),
    ] {
        let r = with_helpers(src);
        assert_eq!(
            e2011(&r),
            1,
            "{label} must reject with HYB-E2011: {:?}",
            r.diagnostics
        );
        assert_eq!(r.verdict, Verdict::Reject);
    }
}

#[test]
fn unshadowed_try_after_inner_shadow_scope_exits_stays_accepted() {
    for (label, src) in [
        (
            "try after inner writer-shadowing block exits in http.handler closure",
            r#"import "net/http"

fn main() {
    mux := http.new_serve_mux()
    mux.handle("/x", http.handler(fn(w, r) {
        if true {
            w := 1
            _ = w
        }
        x := val()?
        _ = x
        _ = w
        _ = r
    }))
}
"#,
        ),
        (
            "try after inner http-shadowing block exits in http.handler closure",
            r#"import "net/http"

fn main() {
    mux := http.new_serve_mux()
    mux.handle("/x", http.handler(fn(w, r) {
        if true {
            http := 1
            _ = http
        }
        x := val()?
        _ = x
        _ = w
        _ = r
    }))
}
"#,
        ),
        (
            "generic method with unrelated type param [u] returning http.handler",
            r#"import "net/http"

struct holder[t] {
    v t
}

fn (h holder[u]) make_h() http.handler {
    _ = h.v
    return fn(w, r) {
        close_it()?
        _ = w
        _ = r
    }
}

fn main() {
    mux := http.new_serve_mux()
    b := holder[int]{v: 1}
    mux.handle("/x", b.make_h())
}
"#,
        ),
        (
            "generic fn with unrelated type param [t] returning (t, error)",
            r#"fn ok[t](x t) (t, error) {
    close_it()?
    return x, nil
}

fn main() {
    _, _ = ok(42)
}
"#,
        ),
    ] {
        let r = with_helpers(src);
        assert_eq!(
            e2011(&r),
            0,
            "{label} must accept unshadowed `?`: {:?}",
            r.diagnostics
        );
        assert!(
            r.diagnostics.is_empty(),
            "{label} must emit zero diagnostics: {:?}",
            r.diagnostics
        );
        assert_eq!(r.verdict, Verdict::Accept);
    }
}
