//! W-016 M3 — the A5 Go-escape check-site (`HYB-E3013`).
//!
//! SPEC-LANGUAGE §8.5 C9 (decided W-016 M2): a closure that writes a captured
//! mutable binding and is passed as an argument to a package-qualified foreign
//! Go call is a check-site. The closed-world rule needs no curated Go API
//! database — it is decidable from Hybrid source alone:
//!   1. the callee is `<pkg>.<fn>` with `<pkg>` an imported package, and
//!   2. an argument is a `fn(...) { ... }` literal that writes a binding it
//!      captured (a place root not declared inside the literal).
//!
//! The accept cases are the point of this file: a read-only capture, a write to
//! a closure-local, a foreign call with no closure, and a plain Hybrid fn call
//! must all stay green.

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("parses");
    check_file(&ast)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn assert_rejected(src: &str, want: &str) {
    let cs = codes(src);
    assert!(
        cs.iter().any(|c| c == want),
        "expected [{want}] among diagnostics, got {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(cs.is_empty(), "expected 0 diagnostics, got {cs:?} for:\n{src}");
}

const IMPORT: &str = "import \"net/http\"\n";

#[test]
fn captured_write_into_foreign_call_is_rejected() {
    let src = format!(
        "{IMPORT}\nfn main() {{\n    hits := 0\n    mux := http.new_serve_mux()\n    mux.handle(\"/x\", http.handler(fn() {{ hits++ }}))\n}}\n"
    );
    assert_rejected(&src, "HYB-E3013");
}

#[test]
fn read_only_capture_into_foreign_call_is_accepted() {
    let src = format!(
        "{IMPORT}\nfn main() {{\n    hits := 0\n    mux := http.new_serve_mux()\n    mux.handle(\"/x\", http.handler(fn() {{ fmt.println(hits) }}))\n}}\n"
    );
    assert_ok(&src);
}

#[test]
fn write_to_a_closure_local_is_accepted() {
    let src = format!(
        "{IMPORT}\nfn main() {{\n    mux := http.new_serve_mux()\n    mux.handle(\"/x\", http.handler(fn() {{\n        local := 0\n        local++\n        fmt.println(local)\n    }}))\n}}\n"
    );
    assert_ok(&src);
}

#[test]
fn foreign_call_without_a_closure_is_accepted() {
    let src = format!(
        "{IMPORT}\nfn main() {{\n    mux := http.new_serve_mux()\n    mux.handle(\"/x\", 1)\n}}\n"
    );
    assert_ok(&src);
}

#[test]
fn a_plain_function_call_is_not_a_foreign_site() {
    // `worker` is a local Hybrid fn, not a package-qualified callee.
    let src =
        "fn worker(f fn()) {\n    f()\n}\n\nfn main() {\n    hits := 0\n    worker(fn() { hits++ })\n}\n";
    assert_ok(src);
}