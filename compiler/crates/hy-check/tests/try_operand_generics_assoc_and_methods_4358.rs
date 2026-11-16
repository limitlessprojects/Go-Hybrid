//! Regression (#4358): `check_try_operand` must enforce `HYB-E2011` across
//! generic calls, associated functions, user struct methods, and `fn`-typed
//! bindings/parameters/struct fields when results do not end in `error`
//! (or return `option[T]`), while accepting valid error-returning forms.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("inline.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn e2011(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error && d.code == "HYB-E2011")
        .map(|d| d.message.clone())
        .collect()
}

fn assert_reject_e2011(src: &str, needle: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{src}");
    let msgs = e2011(&r);
    assert!(
        msgs.iter().any(|m| m.contains(needle)),
        "expected HYB-E2011 containing {needle:?}, got {msgs:?}\n{src}"
    );
}

#[test]
fn generic_assoc_method_and_fn_bindings_reject_non_error_try() {
    // 1. Generic free function call
    assert_reject_e2011(
        "fn id[t any](x t) t { return x }\nfn main() {\n    v := id[int](42)?\n    fmt.println(\"{v}\")\n}\n",
        "do not end in `error`",
    );

    // 2. Associated function call (and generic associated function)
    assert_reject_e2011(
        "struct box { n int }\nfn box.make(n int) box { return box{n: n} }\nfn main() {\n    b := box.make(1)?\n    _ = b\n}\n",
        "do not end in `error`",
    );
    assert_reject_e2011(
        "struct cache[k comparable, v any] { n int }\nfn cache.make[k comparable, v any](n int) option[int] { return none }\nfn main() {\n    c := cache.make[string, int](8)?\n    _ = c\n}\n",
        "on `option`",
    );

    // 3. User struct method calls (binding, composite literal, nested field)
    assert_reject_e2011(
        "struct counter { n int }\nstruct holder { inner counter }\nfn (c counter) value() int { return c.n }\nfn main() {\n    h := holder{inner: counter{n: 5}}\n    a := h.inner.value()?\n    b := (counter{n: 2}).value()?\n    _ = a\n    _ = b\n}\n",
        "do not end in `error`",
    );

    // 4. fn-typed parameters, local closure bindings, and struct fields
    assert_reject_e2011(
        "fn run(cb fn() int) error {\n    x := cb()?\n    _ = x\n    return nil\n}\nfn main() { _ = run(fn() int { return 1 }) }\n",
        "do not end in `error`",
    );
    assert_reject_e2011(
        "fn main() {\n    local_cb := fn() option[int] { return none }\n    x := local_cb()?\n    _ = x\n}\n",
        "on `option`",
    );
    assert_reject_e2011(
        "struct runner { hook fn() int }\nfn main() {\n    r := runner{hook: fn() int { return 7 }}\n    x := r.hook()?\n    _ = x\n}\n",
        "do not end in `error`",
    );
}

#[test]
fn valid_error_returning_generics_assoc_methods_and_fn_bindings_accept() {
    let r = check(
        "struct counter { n int }\n\
         struct runner { hook fn() error }\n\
         fn id_err[t any](x t) (t, error) { return x, nil }\n\
         fn counter.open(n int) (counter, error) { return counter{n: n}, nil }\n\
         fn (c counter) read() (int, error) { return c.n, nil }\n\
         fn run(cb fn() error) error {\n\
             cb()?\n\
             c := counter.open(3)?\n\
             v := c.read()?\n\
             g := id_err[int](v)?\n\
             local_cb := fn() (int, error) { return g, nil }\n\
             out := local_cb()?\n\
             _ = out\n\
             r := runner{hook: fn() error { return nil }}\n\
             r.hook()?\n\
             return nil\n\
         }\n\
         fn main() { _ = run(fn() error { return nil }) }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "got {:?}", r.diagnostics);
}
