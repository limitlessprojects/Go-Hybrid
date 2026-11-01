//! E4 (SPEC-LANGUAGE §6.1) — where `?` may appear: a fn or closure whose
//! results end in `error`, `main` (E6), or an `http.handler` closure (E11).
//! A closure's `?` binds to the closure's own results (lexical, like
//! `return`). Nothing enforced this before HYB-E2011: `?` in a fn returning
//! `int` passed `hyb check` and the emitter refused it, and a result-less
//! closure inside `main` was accepted, built, and kept running after its
//! `?` fired.

use hy_check::{check_file, CheckResult};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
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
fn try_in_fn_without_error_result_rejected() {
    let r = with_helpers(
        "fn noerr() int {\n    x := val()?\n    return x\n}\n\nfn main() {\n    fmt.println(noerr())\n}\n",
    );
    assert_eq!(e2011(&r), 1, "{:?}", r.diagnostics);
}

#[test]
fn try_statement_in_void_fn_rejected() {
    let r = with_helpers("fn noerr() {\n    close_it()?\n}\n\nfn main() {\n    noerr()\n}\n");
    assert_eq!(e2011(&r), 1, "{:?}", r.diagnostics);
}

#[test]
fn deferred_try_in_fn_without_error_result_rejected() {
    let r = with_helpers("fn noerr() {\n    defer close_it()?\n}\n\nfn main() {\n    noerr()\n}\n");
    assert_eq!(e2011(&r), 1, "{:?}", r.diagnostics);
}

#[test]
fn result_less_closure_inside_main_rejected() {
    // The accepted-and-built case: the closure's `?` wrote main's exit error
    // and returned from the closure only; main kept running.
    let r = with_helpers(
        "fn main() {\n    step := fn() {\n        x := val()?\n        fmt.println(x)\n    }\n    step()\n    fmt.println(\"main kept running\")\n}\n",
    );
    assert_eq!(e2011(&r), 1, "{:?}", r.diagnostics);
}

#[test]
fn closure_without_error_result_inside_error_fn_rejected() {
    // Lexical: the enclosing fn's `error` result is not the closure's.
    let r = with_helpers(
        "fn outer() error {\n    f := fn() int {\n        x := val()?\n        return x\n    }\n    fmt.println(f())\n    return nil\n}\n\nfn main() {\n    _ = outer()\n}\n",
    );
    assert_eq!(e2011(&r), 1, "{:?}", r.diagnostics);
}

#[test]
fn legal_landing_spots_stay_accepted() {
    for (what, src) in [
        (
            "main (E6)",
            "fn main() {\n    x := val()?\n    fmt.println(x)\n}\n",
        ),
        (
            "error-result fn",
            "fn ok() (int, error) {\n    x := val()?\n    close_it()?\n    return x, nil\n}\n\nfn main() {\n    _, _ = ok()\n}\n",
        ),
        (
            "closure with its own error result",
            "fn main() {\n    f := fn() (int, error) {\n        x := val()?\n        return x, nil\n    }\n    v, _ := f()\n    fmt.println(v)\n}\n",
        ),
        (
            "error-result closure nested in a void fn",
            "fn run() {\n    f := fn() error {\n        close_it()?\n        return nil\n    }\n    _ = f()\n}\n\nfn main() {\n    run()\n}\n",
        ),
        (
            "http.handler closure (E11)",
            "fn main() {\n    mux := http.new_serve_mux()\n    mux.handle(\"/add/\", http.handler(fn(w, r) {\n        a := strconv.atoi(r.url.query.get(\"a\"))?\n        fmt.fprintf(w, \"{a}\\n\")\n    }))\n}\n",
        ),
    ] {
        let r = with_helpers(src);
        assert_eq!(e2011(&r), 0, "{what} must stay legal: {:?}", r.diagnostics);
    }
}

#[test]
fn one_diagnostic_per_misplaced_try_with_e4_note() {
    let r = with_helpers(
        "fn noerr() int {\n    a := val()?\n    b := val()?\n    return a + b\n}\n\nfn main() {\n    fmt.println(noerr())\n}\n",
    );
    assert_eq!(e2011(&r), 2, "{:?}", r.diagnostics);
    let d = r
        .diagnostics
        .iter()
        .find(|d| d.code == "HYB-E2011")
        .unwrap();
    assert!(d.message.contains("`noerr` has none"), "{}", d.message);
}
