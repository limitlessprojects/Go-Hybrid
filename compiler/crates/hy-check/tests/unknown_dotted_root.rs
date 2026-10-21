//! unknown_dotted_root.rs — regression for issue #1131: a dotted access
//! whose ident base is declared nowhere and is not a package root on the
//! M5 curated allowlist passed `hyb check` with 0 diagnostics and then
//! failed the emitted Go with `undefined: widgets` — the emitter collects
//! imports only for curated roots, so an unknown root emits a bare
//! identifier. The landed #658 enforcement (`711e62d`, `05ee796`) covers
//! explicit imports, known uncurated stdlib roots (`math.sqrt`), and
//! unknown members of curated roots (`serde_json.encode`); the never-
//! imported unknown root is the remaining shape.
//!
//! What must reject (HYB-E1001, rule `BUILDABLE`):
//!   * `fmt.println(widgets.count)` — unknown root in a call argument;
//!   * `a.b.c` — unknown innermost root of a chain;
//!   * `widgets.do_thing()` — unknown root as a method-call receiver;
//!   * the same shape inside a closure body.
//!
//! What must stay accepted (all verified OK(0) on main `4f58a7c`):
//!   * field access on a local / parameter / closure parameter / match-arm
//!     binding (any name, including one that looks like a package);
//!   * a receiver method body's field access;
//!   * importless `hex.encode` (emitter usage-driven curated root) and
//!     `fmt.println` / `strings.to_upper` curated members;
//!   * `import "net/http"` + `http.status`;
//!   * `x.b.d` chains on a local root.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
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

fn rejects_with_e1001(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1001"),
        "expected HYB-E1001 for {:?}, got {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

#[test]
fn unknown_root_in_call_argument_is_e1001() {
    rejects_with_e1001("fn main() {\n  fmt.println(widgets.count)\n}\n");
}

#[test]
fn unknown_root_chain_is_e1001() {
    rejects_with_e1001("fn main() {\n  fmt.println(a.b.c)\n}\n");
}

#[test]
fn unknown_root_method_receiver_is_e1001() {
    rejects_with_e1001("fn main() {\n  widgets.do_thing()\n}\n");
}

#[test]
fn unknown_root_in_closure_body_is_e1001() {
    rejects_with_e1001(
        "fn main() {\n\
         \x20 g := fn() {\n\
         \x20   fmt.println(widgets.count)\n\
         \x20 }\n\
         \x20 g()\n\
         }\n",
    );
}

#[test]
fn local_binding_field_access_stays_accepted() {
    let r = check(
        "struct req { url string }\n\
         fn main() {\n\
         \x20 var widgets req\n\
         \x20 fmt.println(widgets.url)\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn parameter_field_access_stays_accepted() {
    let r = check(
        "struct req { url string }\n\
         fn f(widgets req) {\n\
         \x20 fmt.println(widgets.url)\n\
         }\n\
         fn main() { f(req{url: \"x\"}) }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn closure_parameter_field_access_stays_accepted() {
    let r = check(
        "struct req { url string }\n\
         fn main() {\n\
         \x20 g := fn(widgets req) {\n\
         \x20   fmt.println(widgets.url)\n\
         \x20 }\n\
         \x20 g(req{url: \"x\"})\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn match_arm_binding_field_access_stays_accepted() {
    let r = check(
        "struct req { url string }\n\
         fn main() {\n\
         \x20 match 1 {\n\
         \x20   1 => { widgets := req{url: \"a\"}\n\
         \x20     fmt.println(widgets.url) }\n\
         \x20   _ => {}\n\
         \x20 }\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn receiver_body_field_access_stays_accepted() {
    let r = check(
        "struct req { url string }\n\
         fn (w req) get() string { w.url }\n\
         fn main() {\n\
         \x20 r := req{url: \"x\"}\n\
         \x20 fmt.println(r.get())\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn local_chain_field_access_stays_accepted() {
    let r = check(
        "struct c { d int }\n\
         struct a { b c }\n\
         fn main() {\n\
         \x20 x := a{b: c{d: 1}}\n\
         \x20 fmt.println(x.b.d)\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn curated_roots_stay_accepted() {
    for src in [
        "fn main() {\n  fmt.println(hex.encode(\"ab\"))\n}\n",
        "fn main() { fmt.println(1) }\n",
        "import \"strings\"\nfn main() { fmt.println(strings.to_upper(\"x\")) }\n",
        "import \"io\"\nfn main() { fmt.println(io.eof) }\n",
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "{src}\n{:?}", r.diagnostics);
    }
}

#[test]
fn assignment_to_unknown_root_is_e1001() {
    // The place path already rejects this (pin: no regression, no double
    // code family — same HYB-E1001).
    rejects_with_e1001(
        "struct req { url string }\n\
         fn main() {\n\
         \x20 widgets.count = 1\n\
         }\n",
    );
}
