//! Issue #3467: reject untyped `nil` passed to non-nilable function,
//! method, and closure parameters (Row 13 BUILDABLE).
//!
//! Go refuses untyped nil passed to non-nilable types:
//! - int, float, bool, string
//! - structs
//! - fixed-size arrays [N]T
//!
//! Nilable types stay accepted:
//! - pointers *T
//! - interfaces (any)
//! - slices []T
//! - maps map[K]V
//! - channels chan T
//! - functions fn(...)

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "test.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn errors(r: &hy_check::CheckResult) -> Vec<(String, String)> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| (d.code.clone(), d.message.clone()))
        .collect()
}

fn assert_e1002(src: &str, what: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected Reject for {what}, got Accept (0 diagnostics)"
    );
    let errs = errors(&r);
    assert!(
        errs.iter().any(|(code, _)| code == "HYB-E1002"),
        "expected HYB-E1002 for {what}, got {errs:?}"
    );
}

#[test]
fn rejects_nil_for_non_nilable_function_parameters() {
    assert_e1002(
        "package main\nfn takes_int(x int32) {}\nfn main() { takes_int(nil) }",
        "int32 parameter",
    );
    assert_e1002(
        "package main\nfn takes_float(x float64) {}\nfn main() { takes_float(nil) }",
        "float64 parameter",
    );
    assert_e1002(
        "package main\nfn takes_str(s string) {}\nfn main() { takes_str(nil) }",
        "string parameter",
    );
    assert_e1002(
        "package main\nfn takes_bool(b bool) {}\nfn main() { takes_bool(nil) }",
        "bool parameter",
    );
    assert_e1002(
        "package main\nstruct point { x int32 }\nfn takes_struct(p point) {}\nfn main() { takes_struct(nil) }",
        "struct parameter",
    );
    assert_e1002(
        "package main\nfn takes_arr(a [3]int32) {}\nfn main() { takes_arr(nil) }",
        "[3]int32 array parameter",
    );
}

#[test]
fn rejects_nil_for_method_parameters() {
    let src = r#"
package main

struct service {}
fn (s service) process(x int32) {}

fn main() {
    var s service
    s.process(nil)
}
"#;
    assert_e1002(src, "method parameter");
}

#[test]
fn rejects_nil_for_closure_parameters() {
    let direct_bool = r#"
package main

fn main() {
    (fn(x bool) {})(nil)
}
"#;
    assert_e1002(direct_bool, "direct func literal bool parameter");

    let direct_int = r#"
package main

fn main() {
    (fn(x int32) {})(nil)
}
"#;
    assert_e1002(direct_int, "direct func literal int parameter");
}

#[test]
fn accepts_nil_for_nilable_parameters() {
    let src = r#"
package main

struct node { val int32 }

fn takes_ptr(p *node) {}
fn takes_iface(a any) {}
fn takes_slice(s []int32) {}
fn takes_map(m map[string]int32) {}
fn takes_chan(c chan int32) {}
fn takes_fn(f fn(int32)) {}

fn main() {
    takes_ptr(nil)
    takes_iface(nil)
    takes_slice(nil)
    takes_map(nil)
    takes_chan(nil)
    takes_fn(nil)
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected Accept for nilable parameters, got {:?}",
        r.diagnostics
    );
}
