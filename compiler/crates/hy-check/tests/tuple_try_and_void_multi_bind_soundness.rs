//! Regression tests for T23 (`HYB-E2002`) and binding-count (`HYB-E1002`)
//! soundness across `?`-unwrapped calls, 0-value multi-target bindings, and
//! `const`-closure / `if`-init / `match`-arm closure bindings.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn codes(src: &str) -> (Verdict, Vec<String>) {
    let ast = parse_file("test.hy", src).expect("must parse");
    let r = check_file(&ast);
    let cs = r
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect();
    (r.verdict, cs)
}

fn assert_only(src: &str, want: &str) {
    let (v, cs) = codes(src);
    assert_eq!(v, Verdict::Reject, "expected Reject for:\n{src}");
    assert_eq!(cs, vec![want.to_string()], "for:\n{src}");
}

fn assert_has(src: &str, want: &str) {
    let (v, cs) = codes(src);
    assert_eq!(v, Verdict::Reject, "expected Reject for:\n{src}");
    assert!(cs.iter().any(|c| c == want), "expected {want} in {cs:?} for:\n{src}");
}

fn assert_accept(src: &str) {
    let (v, cs) = codes(src);
    assert!(cs.is_empty(), "expected 0 errors, got {cs:?} for:\n{src}");
    assert_eq!(v, Verdict::Accept);
}

#[test]
fn multi_value_try_unwrap_arity_mismatches_reject_e2002() {
    let hdr = "type my_err = error\n\
               struct box_t { v int }\n\
               fn (b box_t) pair_err() (int, int, error) { return b.v, 2, nil }\n\
               fn pair_err() (int, string, error) { return 1, \"ok\", nil }\n\
               fn alias_pair_err() (int, int, my_err) { return 1, 2, nil }\n";
    for body in [
        "x := pair_err()?\n _ = x",
        "var x int\n x = pair_err()?\n _ = x",
        "a, b, c := pair_err()?\n _ = a\n _ = b\n _ = c",
        "var a int\n var b string\n var c int\n a, b, c = pair_err()?\n _ = a\n _ = b\n _ = c",
        "b := box_t{v: 1}\n x := b.pair_err()?\n _ = x",
        "x := alias_pair_err()?\n _ = x",
        "cb := fn() (int, int, error) { return 1, 2, nil }\n x := cb()?\n _ = x",
    ] {
        let src = format!("{hdr}fn run() error {{\n {body}\n return nil\n}}\nfn main() {{}}\n");
        assert_only(&src, "HYB-E2002");
    }

    // Foreign `?` calls yielding 2 values after unwrap: httptest.get and net.split_host_port.
    assert_only(
        "import \"net/http\"\nimport \"net/http/httptest\"\n\
         fn main() {\n mux := http.new_serve_mux()\n resp := httptest.get(mux, \"/a\")?\n _ = resp\n}\n",
        "HYB-E2002",
    );
    assert_only(
        "import \"net\"\n\
         fn main() {\n x := net.split_host_port(\"127.0.0.1:80\")?\n _ = x\n}\n",
        "HYB-E2002",
    );

    // `return pair_err()?` and `w(pair_err()?)` / `task w(pair_err()?)` hoist to a 1-value temp.
    assert_only(
        &format!("{hdr}fn f() (int, error) {{ return pair_err()? }}\nfn main() {{}}\n"),
        "HYB-E2002",
    );
    assert_only(
        &format!("{hdr}fn f() (int, string, error) {{ return pair_err()? }}\nfn main() {{}}\n"),
        "HYB-E2002",
    );
    assert_has(
        &format!("{hdr}fn w(a int, b string) {{ _ = a\n _ = b }}\nfn run() error {{ w(pair_err()?)\n return nil }}\nfn main() {{}}\n"),
        "HYB-E2002",
    );
    assert_has(
        &format!("{hdr}fn w(a int, b string) {{ _ = a\n _ = b }}\nfn run() error {{ task w(pair_err()?)\n return nil }}\nfn main() {{}}\n"),
        "HYB-E2002",
    );
}

#[test]
fn single_and_zero_value_sources_bound_to_multiple_names_reject_e1002() {
    let hdr = "fn one_err() (int, error) { return 7, nil }\n\
               fn only_err() error { return nil }\n\
               fn void_fn() {}\n";
    for body in [
        "a, b := one_err()?\n _ = a\n _ = b",
        "var a int\n var b int\n a, b = one_err()?\n _ = a\n _ = b",
        "a, b := only_err()?\n _ = a\n _ = b",
        "var a int\n var b int\n a, b = only_err()?\n _ = a\n _ = b",
        "a, b := void_fn()\n _ = a\n _ = b",
        "var a int\n var b int\n a, b = void_fn()\n _ = a\n _ = b",
    ] {
        let src = format!("{hdr}fn run() error {{\n {body}\n return nil\n}}\nfn main() {{}}\n");
        assert_only(&src, "HYB-E1002");
    }
    assert_only(
        "import \"strconv\"\nfn main() {\n a, b := strconv.atoi(\"42\")?\n _ = a\n _ = b\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn const_closure_multi_bind_match_some_and_if_init_closure_shadowing() {
    // 1. Multi-value destructuring inside a file-level `const` closure.
    assert_only(
        "type one_fn = fn() int\ntype two_fn = fn() int int\n\
         fn one() int { return 1 }\nfn two() (int, int) { return 1, 2 }\n\
         fn both() (one_fn, two_fn) { return one, two }\n\
         const cb = [fn() {\n g, f := both()\n _ = g\n x := f()\n _ = x\n}]\n\
         fn main() { _ = cb }\n",
        "HYB-E2002",
    );
    // 2. `match` `some(f)` arm binding inside a file-level `const` closure.
    assert_only(
        "type two_fn = fn() int int\nfn two() (int, int) { return 1, 2 }\n\
         fn opt_two() option[two_fn] { return some(two) }\n\
         const cb = [fn() {\n match opt_two() {\n some(f): { x := f()\n _ = x }\n none: {}\n }\n}]\n\
         fn main() { _ = cb }\n",
        "HYB-E2002",
    );
    // 3. `if` init `?`-unwrapped multi-binding shadowing a 1-value fn in condition.
    assert_only(
        "type one_fn = fn() int\ntype two_fn = fn() int int\n\
         fn one() int { return 1 }\nfn two() (int, int) { return 1, 2 }\n\
         fn both_err() (one_fn, two_fn, error) { return one, two, nil }\n\
         fn main() {\n f := one\n if g, f := both_err()?; g() == f() {}\n _ = f\n}\n",
        "HYB-E2002",
    );
    // 4. Positive controls: matched `?` bindings and shadowed outer 2-value closures.
    assert_accept(
        "import \"net\"\nimport \"strconv\"\n\
         fn pair_err() (int, string, error) { return 1, \"ok\", nil }\n\
         fn main() {\n\
             a, b := pair_err()?\n\
             h, p := net.split_host_port(\"127.0.0.1:80\")?\n\
             n := strconv.atoi(\"42\")?\n\
             f := fn() (int, int) { return 1, 2 }\n\
             if f := fn() int { return 1 }; f() == 1 {}\n\
             _ = a\n _ = b\n _ = h\n _ = p\n _ = n\n _ = f\n\
         }\n",
    );
}
